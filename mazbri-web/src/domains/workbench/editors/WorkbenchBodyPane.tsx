import React, { useEffect, useMemo, useRef, useState } from "react";
import { Upload, Wand2 } from "lucide-react";
import type { ConnectorEndpoint } from "@/api/upstream-connectors";
import {
  Button,
  CodeEditor,
  Input,
  Select,
  cn,
  type SelectOptionItem,
} from "@/components/ui";
import { useT } from "@/locales";
import {
  WRAP_OPTIONS,
  adapterBodyKind,
  bodyAsFormRows,
  bodyToEditorText,
  coerceEndpointJsonBody,
  defaultBodyContentType,
  editorTextToBody,
  ensureEndpointAdapterFields,
  kvToMap,
  listMissingAdapterFields,
  mapToKv,
  prettyJson,
  resolvedBodyEncoding,
  setTenantBodyKind,
  stripEmptyKv,
  type BodyContentType,
  type KvRow,
} from "../engine/workbenchHelpers";
import { WorkbenchKvRows } from "./WorkbenchKvRows";

export interface WorkbenchBodyPaneProps {
  endpoint: ConnectorEndpoint;
  gqlVarRows: KvRow[];
  env?: Record<string, string>;
  uploadingArtifact?: boolean;
  disabled?: boolean;
  maskPreview?: boolean;
  onUpdateEndpoint: (
    updater: (ep: ConnectorEndpoint) => ConnectorEndpoint,
  ) => void;
  onChangeGqlVarRows: (rows: KvRow[]) => void;
  onUploadArtifact: (file: File) => void;
}

function FieldRow({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="grid grid-cols-1 sm:grid-cols-[168px_minmax(0,1fr)] items-center gap-2 border-b border-border-subtle px-3 py-2 last:border-b-0">
      <span className="text-xs font-medium text-muted-foreground">{label}</span>
      <div className="min-w-0">{children}</div>
    </div>
  );
}

function getHeaderValue(ep: ConnectorEndpoint, name: string): string {
  const entries = ep.header_entries;
  if (entries?.length) {
    const hit = entries.find(
      (r) => r.enabled !== false && r.key.toLowerCase() === name.toLowerCase(),
    );
    if (hit) return hit.value;
  }
  const headers = ep.headers || {};
  for (const [k, v] of Object.entries(headers)) {
    if (k.toLowerCase() === name.toLowerCase()) return String(v);
  }
  return "";
}

function withUpdatedHeader(
  ep: ConnectorEndpoint,
  name: string,
  value: string,
): ConnectorEndpoint {
  const trimmed = value.trim();
  const headers = { ...(ep.headers || {}) };
  const existingKey = Object.keys(headers).find(
    (k) => k.toLowerCase() === name.toLowerCase(),
  );
  if (!trimmed) {
    if (existingKey) delete headers[existingKey];
  } else if (existingKey) {
    headers[existingKey] = trimmed;
  } else {
    headers[name] = trimmed;
  }

  let headerEntries = ep.header_entries ? [...ep.header_entries] : undefined;
  if (headerEntries?.length) {
    const idx = headerEntries.findIndex(
      (r) => r.key.toLowerCase() === name.toLowerCase(),
    );
    if (!trimmed) {
      if (idx >= 0) headerEntries.splice(idx, 1);
    } else if (idx >= 0) {
      headerEntries[idx] = {
        ...headerEntries[idx]!,
        value: trimmed,
        enabled: true,
      };
    } else {
      headerEntries.push({ key: name, value: trimmed, enabled: true });
    }
  }

  return {
    ...ep,
    headers,
    ...(headerEntries ? { header_entries: headerEntries } : {}),
  };
}

export function WorkbenchBodyPane({
  endpoint,
  gqlVarRows,
  env = {},
  uploadingArtifact = false,
  disabled = false,
  maskPreview = false,
  onUpdateEndpoint,
  onChangeGqlVarRows,
  onUploadArtifact,
}: WorkbenchBodyPaneProps) {
  const { t } = useT();
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const [jsonCoerceFailed, setJsonCoerceFailed] = useState(false);

  const contentType = defaultBodyContentType(endpoint);
  const isFormEncoding =
    contentType === "json" && resolvedBodyEncoding(endpoint) === "form";
  const adapterKind = adapterBodyKind(endpoint.adapter);
  const missingFields = useMemo(
    () => listMissingAdapterFields(endpoint),
    [endpoint],
  );

  const isSoapXmlBody =
    adapterKind === "soap_xml" &&
    typeof endpoint.body === "string" &&
    endpoint.body.trim().startsWith("<");

  const editorText = useMemo(
    () => bodyToEditorText(endpoint.body, isSoapXmlBody ? "xml" : "json"),
    [endpoint.body, isSoapXmlBody],
  );

  const [formRows, setFormRows] = useState<KvRow[]>(() =>
    isFormEncoding ? bodyAsFormRows(endpoint.body) : [],
  );
  const formSyncRef = useRef(false);

  useEffect(() => {
    if (!isFormEncoding) return;
    if (formSyncRef.current) {
      formSyncRef.current = false;
      return;
    }
    setFormRows(bodyAsFormRows(endpoint.body));
  }, [isFormEncoding, endpoint.body]);

  const [soapFieldMapRows, setSoapFieldMapRows] = useState<KvRow[]>(() =>
    adapterKind === "soap_xml"
      ? mapToKv(
          (endpoint.soap?.field_map || {}) as Record<string, string>,
        )
      : [],
  );
  const soapSyncRef = useRef(false);

  useEffect(() => {
    if (adapterKind !== "soap_xml") return;
    if (soapSyncRef.current) {
      soapSyncRef.current = false;
      return;
    }
    setSoapFieldMapRows(
      mapToKv((endpoint.soap?.field_map || {}) as Record<string, string>),
    );
  }, [adapterKind, endpoint.soap]);

  const contentTypeOptions: SelectOptionItem[] = useMemo(
    () => [
      {
        value: "none",
        label: t("upstreamConnector.bodyTypeNone"),
        text: t("upstreamConnector.bodyTypeNone"),
      },
      {
        value: "json",
        label: t("upstreamConnector.bodyTypeJson"),
        text: t("upstreamConnector.bodyTypeJson"),
      },
    ],
    [t],
  );

  const wrapOptions: SelectOptionItem[] = useMemo(
    () =>
      WRAP_OPTIONS.map((w) => {
        const key = `upstreamConnector.wrapNames.${w}`;
        const label = t(key);
        const text = label === key ? w : label;
        return { value: w, label: text, text };
      }),
    [t],
  );

  const wsAuthInOptions: SelectOptionItem[] = useMemo(
    () => [
      {
        value: "",
        label: t("upstreamConnector.wsAuthInNone"),
        text: t("upstreamConnector.wsAuthInNone"),
      },
      {
        value: "header",
        label: t("upstreamConnector.injectToHeader"),
        text: t("upstreamConnector.injectToHeader"),
      },
      {
        value: "query",
        label: t("upstreamConnector.injectToQuery"),
        text: t("upstreamConnector.injectToQuery"),
      },
      {
        value: "first_frame",
        label: t("upstreamConnector.wsAuthInFirstFrame"),
        text: t("upstreamConnector.wsAuthInFirstFrame"),
      },
    ],
    [t],
  );

  const handleContentTypeChange = (value: string) => {
    const nextKind: BodyContentType = value === "none" ? "none" : "json";
    let coerceOk = true;
    onUpdateEndpoint((ep) => {
      const next = { ...ep };
      setTenantBodyKind(next, nextKind);
      if (nextKind === "json") {
        coerceOk = coerceEndpointJsonBody(next);
      }
      return next;
    });
    setJsonCoerceFailed(nextKind === "json" ? !coerceOk : false);
  };

  const handleEditorChange = (nextText: string) => {
    if (adapterKind === "soap_xml" && nextText.trim().startsWith("<")) {
      setJsonCoerceFailed(false);
      onUpdateEndpoint((ep) => ({ ...ep, body: nextText }));
      return;
    }
    const parsed = editorTextToBody(nextText, "json");
    setJsonCoerceFailed(
      parsed !== undefined &&
        (typeof parsed !== "object" ||
          parsed === null ||
          Array.isArray(parsed)),
    );
    onUpdateEndpoint((ep) => ({ ...ep, body: parsed }));
  };

  const handleFormatJson = () => {
    const formatted = prettyJson(editorText);
    if (!formatted) return;
    handleEditorChange(formatted);
  };

  const handleFormRowsChange = (nextRows: KvRow[]) => {
    formSyncRef.current = true;
    setFormRows(nextRows);
    const bodyObj = kvToMap(stripEmptyKv(nextRows));
    onUpdateEndpoint((ep) => ({ ...ep, body: bodyObj }));
  };

  const handleSoapFieldMapChange = (nextRows: KvRow[]) => {
    soapSyncRef.current = true;
    setSoapFieldMapRows(nextRows);
    const fm = kvToMap(stripEmptyKv(nextRows));
    onUpdateEndpoint((ep) => {
      const next = ensureEndpointAdapterFields({ ...ep });
      next.soap = { ...(next.soap || {}), field_map: fm };
      return next;
    });
  };

  const jobField = (key: string): string => {
    const raw = (endpoint.job_poll || {}) as Record<string, unknown>;
    return String(raw[key] ?? "");
  };

  const setJobField = (key: string, value: string) => {
    onUpdateEndpoint((ep) => {
      const next = ensureEndpointAdapterFields({ ...ep });
      next.job_poll = { ...(next.job_poll || {}), [key]: value };
      return next;
    });
  };

  const gqlDocument = String(endpoint.graphql?.document ?? "");
  const gqlOperation = String(endpoint.graphql?.operation_name ?? "");
  const soapAction = getHeaderValue(endpoint, "SOAPAction");
  const soapNumberFieldsText = Array.isArray(endpoint.soap?.number_fields)
    ? (endpoint.soap?.number_fields as unknown[]).map(String).join(", ")
    : "";
  const wsPath = String(
    (endpoint.ws as { path?: string } | undefined)?.path ?? "",
  );
  const wsAuthInList = (endpoint.ws as { auth_in?: string[] } | undefined)
    ?.auth_in;
  const wsAuthInPrimary =
    Array.isArray(wsAuthInList) && wsAuthInList.length
      ? String(wsAuthInList[0] || "")
      : "";

  return (
    <div className="flex h-full min-h-0 flex-col gap-3">
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border-subtle pb-2">
        <div className="flex items-center gap-2">
          <span className="text-xs font-semibold text-muted-foreground">
            {t("upstreamConnector.bodyContentType")}
          </span>
          <Select
            value={contentType}
            options={contentTypeOptions}
            disabled={disabled}
            aria-label={t("upstreamConnector.bodyContentType")}
            onChange={(e) => handleContentTypeChange(e.target.value)}
            className="w-40"
          />
        </div>

        {contentType === "json" && !isFormEncoding && !isSoapXmlBody && (
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={disabled || !editorText.trim()}
            onClick={handleFormatJson}
          >
            <Wand2 className="h-3.5 w-3.5" />
            {t("supplier.jsonTemplate.formatJson")}
          </Button>
        )}
      </div>

      {contentType === "none" ? (
        <div className="flex min-h-[120px] flex-1 items-center justify-center rounded-lg border border-dashed border-border px-4 py-8 text-center text-xs text-muted-foreground">
          {t("upstreamConnector.bodyNoneEmpty")}
        </div>
      ) : (
        <div className="flex flex-col gap-2">
          {jsonCoerceFailed && (
            <div className="rounded-md border border-danger-fg/35 bg-danger-fg/10 px-3 py-1.5 text-xs text-danger-fg">
              {t("upstreamConnector.bodyNeedsJsonObject")}
            </div>
          )}

          {isFormEncoding ? (
            <WorkbenchKvRows
              rows={formRows}
              onChange={handleFormRowsChange}
              title={t("upstreamConnector.formBody")}
              addTitle={t("upstreamConnector.addQuery")}
              clearTitle={t("upstreamConnector.clearKv")}
              env={env}
              disabled={disabled}
              maskPreview={maskPreview}
            />
          ) : (
            <CodeEditor
              value={editorText}
              onChange={handleEditorChange}
              language={isSoapXmlBody ? "xml" : "json"}
              readOnly={disabled}
              minHeight={240}
              placeholder={isSoapXmlBody ? "<Envelope />" : "{}"}
            />
          )}
        </div>
      )}

      {missingFields.length > 0 && (
        <div className="rounded-lg border border-danger-fg/35 bg-danger-fg/8 p-3 space-y-1">
          <div className="text-xs font-medium text-danger-fg">
            {t("upstreamConnector.adapterFieldsMissing")}
          </div>
          <ul className="list-disc pl-5 text-xs text-foreground space-y-0.5">
            {missingFields.map((m) => (
              <li key={m.path}>
                <code className="font-mono-tech text-[11px]">{m.path}</code> —{" "}
                {t(m.labelKey)}
              </li>
            ))}
          </ul>
        </div>
      )}

      {adapterKind === "graphql" && (
        <div className="rounded-lg border border-border bg-card overflow-hidden">
          <div className="border-b border-border-subtle bg-muted/40 px-3 py-2 text-xs font-semibold text-muted-foreground">
            {t("upstreamConnector.adapterParamsSection")}
          </div>
          <div className="p-3 space-y-3">
            <CodeEditor
              value={gqlDocument}
              onChange={(val) =>
                onUpdateEndpoint((ep) => {
                  const next = ensureEndpointAdapterFields({ ...ep });
                  next.graphql = { ...(next.graphql || {}), document: val };
                  return next;
                })
              }
              language="text"
              readOnly={disabled}
              minHeight={160}
              placeholder="query { }"
            />
            <div className="rounded-lg border border-border-subtle">
              <FieldRow label={t("upstreamConnector.graphqlOperationName")}>
                <Input
                  value={gqlOperation}
                  disabled={disabled}
                  placeholder="GetOrder"
                  aria-label={t("upstreamConnector.graphqlOperationName")}
                  onChange={(e) => {
                    const val = e.target.value;
                    onUpdateEndpoint((ep) => {
                      const next = ensureEndpointAdapterFields({ ...ep });
                      next.graphql = {
                        ...(next.graphql || {}),
                        operation_name: val,
                      };
                      return next;
                    });
                  }}
                />
              </FieldRow>
            </div>
            <WorkbenchKvRows
              rows={gqlVarRows}
              onChange={onChangeGqlVarRows}
              title={t("upstreamConnector.graphqlVariablesMap")}
              addTitle={t("upstreamConnector.addQuery")}
              clearTitle={t("upstreamConnector.clearKv")}
              env={env}
              disabled={disabled}
              maskPreview={maskPreview}
            />
          </div>
        </div>
      )}

      {adapterKind === "job_poll" && (
        <div className="rounded-lg border border-border bg-card overflow-hidden">
          <div className="border-b border-border-subtle bg-muted/40 px-3 py-2 text-xs font-semibold text-muted-foreground">
            {t("upstreamConnector.adapterParamsSection")}
          </div>
          <div className="px-3 pt-2.5">
            <div className="rounded-md border border-danger-fg/35 bg-danger-fg/8 px-2.5 py-1.5 text-xs text-danger-fg">
              {t("upstreamConnector.jobPollAsyncHint")}
            </div>
          </div>
          <div className="mt-1">
            <FieldRow label={t("upstreamConnector.jobSubmitPath")}>
              <Input
                value={jobField("submit_path")}
                disabled={disabled}
                placeholder={endpoint.path || "/"}
                aria-label={t("upstreamConnector.jobSubmitPath")}
                className="font-mono-tech"
                onChange={(e) => setJobField("submit_path", e.target.value)}
              />
            </FieldRow>
            <FieldRow label={t("upstreamConnector.jobQueryPath")}>
              <Input
                value={jobField("query_path")}
                disabled={disabled}
                placeholder="/jobs/{job_id}"
                aria-label={t("upstreamConnector.jobQueryPath")}
                className="font-mono-tech"
                onChange={(e) => setJobField("query_path", e.target.value)}
              />
            </FieldRow>
            <FieldRow label={t("upstreamConnector.jobIdPath")}>
              <Input
                value={jobField("job_id_jsonpath")}
                disabled={disabled}
                placeholder="$.job_id"
                aria-label={t("upstreamConnector.jobIdPath")}
                className="font-mono-tech"
                onChange={(e) => setJobField("job_id_jsonpath", e.target.value)}
              />
            </FieldRow>
          </div>
        </div>
      )}

      {adapterKind === "grpc" && (
        <div className="rounded-lg border border-border bg-card overflow-hidden">
          <div className="border-b border-border-subtle bg-muted/40 px-3 py-2 text-xs font-semibold text-muted-foreground">
            {t("upstreamConnector.adapterParamsSection")}
          </div>
          <FieldRow label={t("upstreamConnector.artifactId")}>
            <div className="flex items-center gap-2">
              <Input
                value={endpoint.artifact_id || ""}
                disabled={disabled}
                placeholder="uuid"
                aria-label={t("upstreamConnector.artifactId")}
                className="font-mono-tech"
                onChange={(e) => {
                  const val = e.target.value;
                  onUpdateEndpoint((ep) => ({ ...ep, artifact_id: val }));
                }}
              />
              <Button
                type="button"
                variant="outline"
                size="md"
                loading={uploadingArtifact}
                disabled={disabled || uploadingArtifact}
                onClick={() => fileInputRef.current?.click()}
              >
                <Upload className="h-3.5 w-3.5" />
                {t("upstreamConnector.artifactUpload")}
              </Button>
              <input
                ref={fileInputRef}
                type="file"
                accept=".pb,.fds,.bin,.protobuf"
                className="hidden"
                aria-label={t("upstreamConnector.artifactUpload")}
                onChange={(e) => {
                  const file = e.target.files?.[0];
                  e.target.value = "";
                  if (file) onUploadArtifact(file);
                }}
              />
            </div>
          </FieldRow>
        </div>
      )}

      {adapterKind === "http_envelope" && (
        <div className="rounded-lg border border-border bg-card overflow-hidden">
          <div className="border-b border-border-subtle bg-muted/40 px-3 py-2 text-xs font-semibold text-muted-foreground">
            {t("upstreamConnector.adapterParamsSection")}
          </div>
          <FieldRow label={t("upstreamConnector.operation")}>
            <Input
              value={endpoint.operation || ""}
              disabled={disabled}
              placeholder={t("upstreamConnector.operationHintEnvelope")}
              aria-label={t("upstreamConnector.operation")}
              className="font-mono-tech"
              onChange={(e) => {
                const val = e.target.value;
                onUpdateEndpoint((ep) => ({ ...ep, operation: val }));
              }}
            />
          </FieldRow>
          <FieldRow label={t("upstreamConnector.wrap")}>
            <Select
              value={endpoint.wrap || "method_biz_content"}
              options={wrapOptions}
              disabled={disabled}
              aria-label={t("upstreamConnector.wrap")}
              className="w-full"
              onChange={(e) => {
                const val = e.target.value;
                onUpdateEndpoint((ep) => ({ ...ep, wrap: val }));
              }}
            />
          </FieldRow>
          <FieldRow label={t("upstreamConnector.unwrap")}>
            <Input
              value={endpoint.unwrap || ""}
              disabled={disabled}
              placeholder="$.data"
              aria-label={t("upstreamConnector.unwrap")}
              className="font-mono-tech"
              onChange={(e) => {
                const val = e.target.value;
                onUpdateEndpoint((ep) => ({ ...ep, unwrap: val }));
              }}
            />
          </FieldRow>
        </div>
      )}

      {adapterKind === "jsonrpc" && (
        <div className="rounded-lg border border-border bg-card overflow-hidden">
          <div className="border-b border-border-subtle bg-muted/40 px-3 py-2 text-xs font-semibold text-muted-foreground">
            {t("upstreamConnector.adapterParamsSection")}
          </div>
          <FieldRow label={t("upstreamConnector.jsonrpcMethod")}>
            <Input
              value={endpoint.operation || ""}
              disabled={disabled}
              placeholder={t("upstreamConnector.jsonrpcMethodHint")}
              aria-label={t("upstreamConnector.jsonrpcMethod")}
              className="font-mono-tech"
              onChange={(e) => {
                const val = e.target.value;
                onUpdateEndpoint((ep) => ({ ...ep, operation: val }));
              }}
            />
          </FieldRow>
          <p className="px-3 py-2 text-xs text-muted-foreground">
            {t("upstreamConnector.jsonrpcIdHint")}
          </p>
        </div>
      )}

      {adapterKind === "soap_xml" && (
        <div className="rounded-lg border border-border bg-card overflow-hidden">
          <div className="border-b border-border-subtle bg-muted/40 px-3 py-2 text-xs font-semibold text-muted-foreground">
            {t("upstreamConnector.adapterParamsSection")}
          </div>
          <FieldRow label={t("upstreamConnector.soapOperation")}>
            <Input
              value={endpoint.operation || ""}
              disabled={disabled}
              placeholder={t("upstreamConnector.soapOperationHint")}
              aria-label={t("upstreamConnector.soapOperation")}
              className="font-mono-tech"
              onChange={(e) => {
                const val = e.target.value;
                onUpdateEndpoint((ep) =>
                  ensureEndpointAdapterFields({ ...ep, operation: val }),
                );
              }}
            />
          </FieldRow>
          <FieldRow label={t("upstreamConnector.soapAction")}>
            <Input
              value={soapAction}
              disabled={disabled}
              placeholder={t("upstreamConnector.soapActionHint")}
              aria-label={t("upstreamConnector.soapAction")}
              className="font-mono-tech"
              onChange={(e) => {
                const val = e.target.value;
                onUpdateEndpoint((ep) =>
                  withUpdatedHeader(ep, "SOAPAction", val),
                );
              }}
            />
          </FieldRow>
          <div className="border-b border-border-subtle p-3">
            <WorkbenchKvRows
              rows={soapFieldMapRows}
              onChange={handleSoapFieldMapChange}
              title={t("upstreamConnector.soapFieldMap")}
              addTitle={t("upstreamConnector.addQuery")}
              clearTitle={t("upstreamConnector.clearKv")}
              env={env}
              disabled={disabled}
              maskPreview={maskPreview}
            />
          </div>
          <FieldRow label={t("upstreamConnector.soapNumberFields")}>
            <Input
              value={soapNumberFieldsText}
              disabled={disabled}
              placeholder={t("upstreamConnector.soapNumberFieldsHint")}
              aria-label={t("upstreamConnector.soapNumberFields")}
              className="font-mono-tech"
              onChange={(e) => {
                const fields = e.target.value
                  .split(/[,，\s]+/)
                  .map((s) => s.trim())
                  .filter(Boolean);
                onUpdateEndpoint((ep) => {
                  const next = ensureEndpointAdapterFields({ ...ep });
                  next.soap = { ...(next.soap || {}), number_fields: fields };
                  return next;
                });
              }}
            />
          </FieldRow>
        </div>
      )}

      {adapterKind === "ws_unary" && (
        <div className="rounded-lg border border-border bg-card overflow-hidden">
          <div className="border-b border-border-subtle bg-muted/40 px-3 py-2 text-xs font-semibold text-muted-foreground">
            {t("upstreamConnector.adapterParamsSection")}
          </div>
          <FieldRow label={t("upstreamConnector.operation")}>
            <Input
              value={endpoint.operation || ""}
              disabled={disabled}
              placeholder={t("upstreamConnector.wsOperationHint")}
              aria-label={t("upstreamConnector.operation")}
              className="font-mono-tech"
              onChange={(e) => {
                const val = e.target.value;
                onUpdateEndpoint((ep) => ({ ...ep, operation: val }));
              }}
            />
          </FieldRow>
          <FieldRow label={t("upstreamConnector.wsPath")}>
            <Input
              value={wsPath}
              disabled={disabled}
              placeholder={endpoint.path || "/ws"}
              aria-label={t("upstreamConnector.wsPath")}
              className="font-mono-tech"
              onChange={(e) => {
                const val = e.target.value;
                onUpdateEndpoint((ep) => {
                  const next = ensureEndpointAdapterFields({ ...ep });
                  next.ws = { ...(next.ws || {}), path: val };
                  return next;
                });
              }}
            />
          </FieldRow>
          <FieldRow label={t("upstreamConnector.wrap")}>
            <Select
              value={endpoint.wrap || "none"}
              options={wrapOptions}
              disabled={disabled}
              aria-label={t("upstreamConnector.wrap")}
              className="w-full"
              onChange={(e) => {
                const val = e.target.value;
                onUpdateEndpoint((ep) => ({ ...ep, wrap: val }));
              }}
            />
          </FieldRow>
          <FieldRow label={t("upstreamConnector.wsAuthIn")}>
            <Select
              value={wsAuthInPrimary}
              options={wsAuthInOptions}
              disabled={disabled}
              aria-label={t("upstreamConnector.wsAuthIn")}
              className="w-full"
              onChange={(e) => {
                const trimmed = e.target.value.trim();
                onUpdateEndpoint((ep) => {
                  const next = ensureEndpointAdapterFields({ ...ep });
                  next.ws = {
                    ...(next.ws || {}),
                    auth_in: trimmed ? [trimmed] : [],
                  };
                  return next;
                });
              }}
            />
          </FieldRow>
          <div className="space-y-1 px-3 py-2 text-xs text-muted-foreground">
            <p>{t("upstreamConnector.wsAuthInHint")}</p>
            <p className={cn("text-muted-foreground")}>
              {t("upstreamConnector.wsUnaryPoolHint")}
            </p>
          </div>
        </div>
      )}
    </div>
  );
}
