import React, { useMemo } from "react";
import type { ConnectorEndpoint } from "@/api/upstream-connectors";
import { Input, Select, type SelectOptionItem } from "@/components/ui";
import { useT } from "@/locales";
import {
  EGRESS_ADAPTERS,
  WRAP_OPTIONS,
  resolvedBodyEncoding,
  type KvRow,
} from "../engine/workbenchHelpers";
import { CryptoProfileEditor } from "./CryptoProfileEditor";
import { WorkbenchKvRows } from "./WorkbenchKvRows";

export interface WorkbenchTransformPaneProps {
  endpoint: ConnectorEndpoint;
  gqlEnumRows: KvRow[];
  env?: Record<string, string>;
  disabled?: boolean;
  isSuperAdmin?: boolean;
  credentialKeys?: string[];
  maskPreview?: boolean;
  onUpdateEndpoint: (
    updater: (ep: ConnectorEndpoint) => ConnectorEndpoint,
  ) => void;
  onBodyEncodingChange: (encoding: string) => void;
  onCryptoProfileChange: (value: Record<string, unknown> | null) => void;
  onChangeGqlEnumRows: (rows: KvRow[]) => void;
}

function SheetRow({
  label,
  alignTop = false,
  children,
}: {
  label: string;
  alignTop?: boolean;
  children: React.ReactNode;
}) {
  return (
    <div
      className={`grid grid-cols-1 sm:grid-cols-[168px_minmax(0,1fr)] gap-2 border-b border-border-subtle px-3 py-2.5 last:border-b-0 ${
        alignTop ? "items-start" : "items-center"
      }`}
    >
      <span
        className={`text-xs font-medium text-muted-foreground ${
          alignTop ? "pt-1" : ""
        }`}
      >
        {label}
      </span>
      <div className="min-w-0">{children}</div>
    </div>
  );
}

export function WorkbenchTransformPane({
  endpoint,
  gqlEnumRows,
  env = {},
  disabled = false,
  isSuperAdmin = false,
  credentialKeys = [],
  maskPreview = false,
  onUpdateEndpoint,
  onBodyEncodingChange,
  onCryptoProfileChange,
  onChangeGqlEnumRows,
}: WorkbenchTransformPaneProps) {
  const { t } = useT();
  const adapter = endpoint.adapter || "http_json";

  const adapterOptions: SelectOptionItem[] = useMemo(
    () =>
      EGRESS_ADAPTERS.map((a) => {
        const key = `upstreamConnector.adapterNames.${a}`;
        const label = t(key);
        const text = label === key ? a : label;
        return { value: a, label: text, text };
      }),
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

  const encodingOptions: SelectOptionItem[] = useMemo(
    () => [
      {
        value: "json",
        label: t("upstreamConnector.egressEncodingJson"),
        text: t("upstreamConnector.egressEncodingJson"),
      },
      {
        value: "form",
        label: t("upstreamConnector.egressEncodingForm"),
        text: t("upstreamConnector.egressEncodingForm"),
      },
      {
        value: "none",
        label: t("upstreamConnector.egressEncodingNone"),
        text: t("upstreamConnector.egressEncodingNone"),
      },
    ],
    [t],
  );

  return (
    <div className="flex flex-col gap-3">
      <div className="rounded-lg border border-border bg-card overflow-hidden">
        <SheetRow label={t("upstreamConnector.adapter")}>
          <Select
            value={adapter}
            options={adapterOptions}
            disabled={disabled}
            aria-label={t("upstreamConnector.adapter")}
            className="w-full"
            onChange={(e) => {
              const nextAdapter = e.target.value;
              onUpdateEndpoint((ep) => ({ ...ep, adapter: nextAdapter }));
            }}
          />
        </SheetRow>

        {adapter !== "http_json" && (
          <SheetRow label={t("upstreamConnector.operation")}>
            <Input
              value={endpoint.operation || ""}
              disabled={disabled}
              placeholder={t("upstreamConnector.operationHint")}
              aria-label={t("upstreamConnector.operation")}
              className="font-mono-tech"
              onChange={(e) => {
                const val = e.target.value;
                onUpdateEndpoint((ep) => ({ ...ep, operation: val }));
              }}
            />
          </SheetRow>
        )}

        {(adapter === "http_envelope" || adapter === "ws_unary") && (
          <SheetRow label={t("upstreamConnector.wrap")}>
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
          </SheetRow>
        )}

        {(adapter === "http_envelope" ||
          adapter === "http_json" ||
          adapter === "sign_form") && (
          <SheetRow label={t("upstreamConnector.unwrap")}>
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
          </SheetRow>
        )}

        {(adapter === "http_json" || adapter === "http_envelope") && (
          <SheetRow label={t("upstreamConnector.egressContentType")}>
            <Input
              value={endpoint.content_type || ""}
              disabled={disabled}
              placeholder="application/json"
              aria-label={t("upstreamConnector.egressContentType")}
              className="font-mono-tech"
              onChange={(e) => {
                const val = e.target.value;
                onUpdateEndpoint((ep) => ({ ...ep, content_type: val }));
              }}
            />
          </SheetRow>
        )}

        {adapter === "http_json" && (
          <SheetRow label={t("upstreamConnector.egressBodyEncoding")}>
            <Select
              value={resolvedBodyEncoding(endpoint)}
              options={encodingOptions}
              disabled={disabled}
              aria-label={t("upstreamConnector.egressBodyEncoding")}
              className="w-full"
              onChange={(e) => onBodyEncodingChange(e.target.value)}
            />
          </SheetRow>
        )}

        <SheetRow label={t("upstreamConnector.cryptoSection")} alignTop>
          <CryptoProfileEditor
            value={endpoint.crypto}
            onChange={onCryptoProfileChange}
            disabled={disabled}
            canToggleInsecure={isSuperAdmin}
            credentialKeys={credentialKeys}
          />
        </SheetRow>
      </div>

      {adapter === "graphql" && (
        <WorkbenchKvRows
          rows={gqlEnumRows}
          onChange={onChangeGqlEnumRows}
          emptyCompact
          title={t("upstreamConnector.graphqlEnumMap")}
          addTitle={t("upstreamConnector.addQuery")}
          clearTitle={t("upstreamConnector.clearKv")}
          env={env}
          disabled={disabled}
          maskPreview={maskPreview}
        />
      )}
    </div>
  );
}
