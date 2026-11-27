import React, { useCallback, useMemo, useRef } from "react";
import { EmptyState, cn } from "@/components/ui";
import { useT } from "@/locales";
import {
  selectActiveEp,
  selectActiveEpIdx,
  selectActiveRuntime,
  selectActiveStaticCaptureWarnings,
  selectCanWriteWorkbench,
  selectIsSuperAdmin,
  selectRequiredCredKeys,
  selectRuntimeEnvMap,
  selectSessionCredentialKeys,
  selectUnresolvedRun,
  useWorkbenchStore,
  type RequestSubTabId,
} from "../store";
import { invalidationSource } from "../engine/authInvalidation";
import { AUTH_PROFILES, isSessionHttpAuthActive } from "../engine/authMatrix";
import {
  adapterBodyFilled,
  kvEnabledCount,
  lastAuthJson,
  serializeCaptureList,
} from "../engine/workbenchHelpers";
import { WorkbenchAssertEditor } from "./WorkbenchAssertEditor";
import { WorkbenchAuthPane } from "./WorkbenchAuthPane";
import { WorkbenchBodyPane } from "./WorkbenchBodyPane";
import { WorkbenchCapturesEditor } from "./WorkbenchCapturesEditor";
import type { WorkbenchEnvTextHandle } from "./WorkbenchEnvText";
import { WorkbenchKvRows } from "./WorkbenchKvRows";
import { WorkbenchMetaBar } from "./WorkbenchMetaBar";
import { WorkbenchTlsTimeoutPane } from "./WorkbenchTlsTimeoutPane";
import { WorkbenchTransformPane } from "./WorkbenchTransformPane";
import { WorkbenchUrlBar } from "./WorkbenchUrlBar";

export interface WorkbenchEditorPaneProps {
  urlInputRef?: React.RefObject<WorkbenchEnvTextHandle | null>;
}

interface SubTabItem {
  id: RequestSubTabId;
  label: string;
  count?: number;
  dot?: boolean;
}

export function WorkbenchEditorPane({
  urlInputRef: externalUrlRef,
}: WorkbenchEditorPaneProps = {}) {
  const { t } = useT();
  const internalUrlRef = useRef<WorkbenchEnvTextHandle | null>(null);
  const urlRef = externalUrlRef ?? internalUrlRef;

  const draft = useWorkbenchStore((s) => s.draft);
  const endpoints = useWorkbenchStore((s) => s.endpoints);
  const headerRows = useWorkbenchStore((s) => s.headerRows);
  const queryRows = useWorkbenchStore((s) => s.queryRows);
  const gqlVarRows = useWorkbenchStore((s) => s.gqlVarRows);
  const gqlEnumRows = useWorkbenchStore((s) => s.gqlEnumRows);
  const urlDraft = useWorkbenchStore((s) => s.urlDraft);
  const urlMissing = useWorkbenchStore((s) => s.urlMissing);
  const uploadingArtifact = useWorkbenchStore((s) => s.uploadingArtifact);
  const configForm = useWorkbenchStore((s) => s.configForm);
  const supplierTimeoutMs = useWorkbenchStore((s) => s.supplierTimeoutMs);
  const supplierTlsForm = useWorkbenchStore((s) => s.supplierTlsForm);
  const endpointTlsForm = useWorkbenchStore((s) => s.endpointTlsForm);
  const capturedVars = useWorkbenchStore((s) => s.capturedVars);

  const activeEp = useWorkbenchStore(selectActiveEp);
  const activeEpIdx = useWorkbenchStore(selectActiveEpIdx);
  const activeRuntime = useWorkbenchStore(selectActiveRuntime);
  const runtimeEnvMap = useWorkbenchStore(selectRuntimeEnvMap);
  const unresolved = useWorkbenchStore(selectUnresolvedRun);
  const staticCaptureWarnings = useWorkbenchStore(
    selectActiveStaticCaptureWarnings,
  );
  const requiredCredKeys = useWorkbenchStore(selectRequiredCredKeys);
  const sessionCredentialKeys = useWorkbenchStore(selectSessionCredentialKeys);
  const canWrite = useWorkbenchStore(selectCanWriteWorkbench);
  const isSuperAdmin = useWorkbenchStore(selectIsSuperAdmin);

  const setReqTab = useWorkbenchStore((s) => s.setReqTab);
  const updateActiveEndpoint = useWorkbenchStore((s) => s.updateActiveEndpoint);
  const setHeaderRows = useWorkbenchStore((s) => s.setHeaderRows);
  const setQueryRows = useWorkbenchStore((s) => s.setQueryRows);
  const setGqlVarRows = useWorkbenchStore((s) => s.setGqlVarRows);
  const setGqlEnumRows = useWorkbenchStore((s) => s.setGqlEnumRows);
  const setUrlFocused = useWorkbenchStore((s) => s.setUrlFocused);
  const onUrlInput = useWorkbenchStore((s) => s.onUrlInput);
  const onUrlBlur = useWorkbenchStore((s) => s.onUrlBlur);
  const applyCurlPaste = useWorkbenchStore((s) => s.applyCurlPaste);
  const setConfigForm = useWorkbenchStore((s) => s.setConfigForm);
  const setSupplierTimeoutMs = useWorkbenchStore((s) => s.setSupplierTimeoutMs);
  const setSupplierTlsForm = useWorkbenchStore((s) => s.setSupplierTlsForm);
  const setEndpointTlsForm = useWorkbenchStore((s) => s.setEndpointTlsForm);
  const setEpAuthProfile = useWorkbenchStore((s) => s.setEpAuthProfile);
  const onCryptoProfileChange = useWorkbenchStore(
    (s) => s.onCryptoProfileChange,
  );
  const onBodyEncodingChange = useWorkbenchStore((s) => s.onBodyEncodingChange);
  const onAdapterQuickSelect = useWorkbenchStore((s) => s.onAdapterQuickSelect);
  const uploadArtifactFile = useWorkbenchStore((s) => s.uploadArtifactFile);
  const clearCapturedVars = useWorkbenchStore((s) => s.clearCapturedVars);
  const removeCapturedVar = useWorkbenchStore((s) => s.removeCapturedVar);
  const doRun = useWorkbenchStore((s) => s.doRun);
  const doLiveRun = useWorkbenchStore((s) => s.doLiveRun);
  const doTransformPreview = useWorkbenchStore((s) => s.doTransformPreview);
  const abortActiveRun = useWorkbenchStore((s) => s.abortActiveRun);
  const copyCurl = useWorkbenchStore((s) => s.copyCurl);

  const focusUrlField = useCallback(() => {
    requestAnimationFrame(() => urlRef.current?.focus());
  }, [urlRef]);

  const queryCount = useMemo(() => kvEnabledCount(queryRows), [queryRows]);
  const headerCount = useMemo(() => kvEnabledCount(headerRows), [headerRows]);
  const captureCount = useMemo(
    () => serializeCaptureList(activeEp?.captures).length,
    [activeEp?.captures],
  );

  const bodyFilled = useMemo(
    () => (activeEp ? adapterBodyFilled(activeEp) : false),
    [activeEp],
  );

  const authFilled = useMemo(() => {
    if (
      isSessionHttpAuthActive(
        configForm.auth_flow.enabled,
        configForm.auth_flow.profile,
      )
    ) {
      return true;
    }
    const profile = String(activeEp?.auth?.profile || "none");
    return (
      (AUTH_PROFILES as readonly string[]).includes(profile) &&
      profile !== "none"
    );
  }, [configForm.auth_flow.enabled, configForm.auth_flow.profile, activeEp?.auth?.profile]);

  const transformFilled = useMemo(() => {
    if (!activeEp) return false;
    if ((activeEp.adapter || "http_json") !== "http_json") return true;
    if ((activeEp.operation || "").trim()) return true;
    if ((activeEp.unwrap || "").trim()) return true;
    const wrap = (activeEp.wrap || "").trim();
    if (Boolean(wrap) && wrap !== "none") return true;
    return Boolean(activeEp.crypto && typeof activeEp.crypto === "object");
  }, [activeEp]);

  const assertFilled = useMemo(
    () => (activeEp?.assertions || []).some((a) => (a.type || "").trim()),
    [activeEp?.assertions],
  );

  const tlsFilled = useMemo(
    () =>
      Boolean(
        supplierTlsForm.enabled ||
          endpointTlsForm.enabled ||
          (activeEp?.timeout_ms && activeEp.timeout_ms > 0) ||
          (activeEp?.base_url || "").trim() ||
          (activeEp?.request_id_header || "").trim(),
      ),
    [
      supplierTlsForm.enabled,
      endpointTlsForm.enabled,
      activeEp?.timeout_ms,
      activeEp?.base_url,
      activeEp?.request_id_header,
    ],
  );

  const activeInvalidationSource = useMemo(
    () =>
      invalidationSource(
        (activeEp?.auth as Record<string, unknown> | null | undefined) ?? null,
        configForm.auth_flow,
      ),
    [activeEp?.auth, configForm.auth_flow],
  );

  const authSeedJson = useMemo(
    () => lastAuthJson(activeRuntime.lastRun),
    [activeRuntime.lastRun],
  );

  const subTabs = useMemo<SubTabItem[]>(
    () => [
      {
        id: "params",
        label: t("upstreamConnector.paramsTab"),
        count: queryCount,
      },
      {
        id: "body",
        label: t("upstreamConnector.bodyTab"),
        dot: bodyFilled,
      },
      {
        id: "headers",
        label: t("upstreamConnector.headersTab"),
        count: headerCount,
      },
      {
        id: "auth",
        label: t("upstreamConnector.authTab"),
        dot: authFilled,
      },
      {
        id: "transform",
        label: t("upstreamConnector.transformTab"),
        dot: transformFilled,
      },
      {
        id: "assert",
        label: t("upstreamConnector.assertTab"),
        dot: assertFilled,
      },
      {
        id: "captures",
        label: t("upstreamConnector.capturesTab"),
        count: captureCount,
      },
      {
        id: "tls",
        label: t("upstreamConnector.tlsTab"),
        dot: tlsFilled,
      },
    ],
    [
      t,
      queryCount,
      bodyFilled,
      headerCount,
      authFilled,
      transformFilled,
      assertFilled,
      captureCount,
      tlsFilled,
    ],
  );

  if (!activeEp) {
    return (
      <div className="flex h-full items-center justify-center p-4">
        <EmptyState
          title={t("upstreamConnector.noEndpoints")}
          className="w-full max-w-md"
        />
      </div>
    );
  }

  const reqTab = activeRuntime.reqTab;

  return (
    <div className="flex h-full min-h-0 flex-col bg-card">
      <WorkbenchMetaBar
        endpoint={activeEp}
        endpoints={endpoints}
        activeIndex={activeEpIdx}
        canWrite={canWrite}
        onUpdateEndpoint={updateActiveEndpoint}
        onSelectAdapter={onAdapterQuickSelect}
        onEditAdapter={() => setReqTab("transform")}
      />

      <WorkbenchUrlBar
        method={activeEp.method || "GET"}
        urlDraft={urlDraft}
        urlMissing={urlMissing}
        running={activeRuntime.running}
        canWrite={canWrite}
        hasLinkedSupplier={Boolean(draft?.linked_supplier_id)}
        env={runtimeEnvMap}
        unresolved={unresolved}
        staticCaptureWarnings={staticCaptureWarnings}
        urlInputRef={urlRef}
        onMethodChange={(method) =>
          updateActiveEndpoint((ep) => ({ ...ep, method }))
        }
        onUrlFocus={() => setUrlFocused(true)}
        onUrlInput={onUrlInput}
        onUrlBlur={onUrlBlur}
        onCurlPaste={applyCurlPaste}
        onSend={() => {
          void doRun("business", focusUrlField);
        }}
        onCancel={abortActiveRun}
        onLiveRun={() => {
          void doLiveRun();
        }}
        onRunAuth={() => {
          void doRun("auth_only", focusUrlField);
        }}
        onTransformPreview={() => {
          void doTransformPreview();
        }}
        onCopyCurl={() => {
          void copyCurl();
        }}
      />

      <div
        role="tablist"
        className="flex shrink-0 items-center gap-1 overflow-x-auto border-b border-border bg-card px-3"
      >
        {subTabs.map((tab) => {
          const active = reqTab === tab.id;
          return (
            <button
              key={tab.id}
              type="button"
              role="tab"
              aria-selected={active}
              onClick={() => setReqTab(tab.id)}
              className={cn(
                "inline-flex h-8 shrink-0 items-center gap-1.5 border-b-2 px-2.5 text-xs font-medium whitespace-nowrap transition-colors -mb-px cursor-pointer",
                active
                  ? "border-primary text-primary font-semibold"
                  : "border-transparent text-muted-foreground hover:text-foreground hover:bg-hover-bg/60",
              )}
            >
              <span>{tab.label}</span>
              {typeof tab.count === "number" && tab.count > 0 && (
                <span
                  className={cn(
                    "rounded-full px-1.5 py-0 text-[10px] font-mono-tech tabular-nums leading-4",
                    active
                      ? "bg-primary-soft text-primary"
                      : "bg-muted text-muted-foreground",
                  )}
                >
                  {tab.count}
                </span>
              )}
              {tab.dot && (
                <span
                  className="h-1.5 w-1.5 rounded-full bg-success-fg shrink-0"
                  aria-hidden="true"
                />
              )}
            </button>
          );
        })}
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto p-3">
        {reqTab === "params" && (
          <WorkbenchKvRows
            rows={queryRows}
            onChange={setQueryRows}
            title={t("upstreamConnector.queryList")}
            addTitle={t("upstreamConnector.addQuery")}
            clearTitle={t("upstreamConnector.clearKv")}
            env={runtimeEnvMap}
            disabled={!canWrite}
            maskPreview={!canWrite}
          />
        )}

        {reqTab === "body" && (
          <WorkbenchBodyPane
            endpoint={activeEp}
            gqlVarRows={gqlVarRows}
            env={runtimeEnvMap}
            uploadingArtifact={uploadingArtifact}
            disabled={!canWrite}
            maskPreview={!canWrite}
            onUpdateEndpoint={updateActiveEndpoint}
            onChangeGqlVarRows={setGqlVarRows}
            onUploadArtifact={(file) => {
              void uploadArtifactFile(file);
            }}
          />
        )}

        {reqTab === "headers" && (
          <WorkbenchKvRows
            rows={headerRows}
            onChange={setHeaderRows}
            variant="headers"
            title={t("upstreamConnector.headersList")}
            addTitle={t("upstreamConnector.addHeader")}
            clearTitle={t("upstreamConnector.clearKv")}
            env={runtimeEnvMap}
            disabled={!canWrite}
            maskPreview={!canWrite}
          />
        )}

        {reqTab === "auth" && (
          <WorkbenchAuthPane
            profile={String(activeEp.auth?.profile || "none")}
            flow={configForm.auth_flow}
            adapter={String(activeEp.adapter || "http_json")}
            invalidationSource={activeInvalidationSource}
            env={runtimeEnvMap}
            seedJson={authSeedJson}
            running={activeRuntime.running}
            disabled={!canWrite}
            requiredCredKeys={requiredCredKeys}
            onChangeProfile={setEpAuthProfile}
            onUpdateFlow={(updater) =>
              setConfigForm((prev) => ({
                ...prev,
                auth_flow: updater(prev.auth_flow),
              }))
            }
            onRunAuth={() => {
              void doRun("auth_only", focusUrlField);
            }}
          />
        )}

        {reqTab === "transform" && (
          <WorkbenchTransformPane
            endpoint={activeEp}
            gqlEnumRows={gqlEnumRows}
            env={runtimeEnvMap}
            disabled={!canWrite}
            isSuperAdmin={isSuperAdmin}
            credentialKeys={sessionCredentialKeys}
            maskPreview={!canWrite}
            onUpdateEndpoint={updateActiveEndpoint}
            onBodyEncodingChange={onBodyEncodingChange}
            onCryptoProfileChange={onCryptoProfileChange}
            onChangeGqlEnumRows={setGqlEnumRows}
          />
        )}

        {reqTab === "assert" && (
          <WorkbenchAssertEditor
            assertions={activeEp.assertions || []}
            disabled={!canWrite}
            onChange={(assertions) =>
              updateActiveEndpoint((ep) => ({ ...ep, assertions }))
            }
          />
        )}

        {reqTab === "captures" && (
          <WorkbenchCapturesEditor
            captures={activeEp.captures || []}
            capturedVars={capturedVars}
            disabled={!canWrite}
            onChange={(captures) =>
              updateActiveEndpoint((ep) => ({ ...ep, captures }))
            }
            onClearCaptured={clearCapturedVars}
            onRemoveCaptured={removeCapturedVar}
          />
        )}

        {reqTab === "tls" && (
          <WorkbenchTlsTimeoutPane
            endpoint={activeEp}
            supplierTimeoutMs={supplierTimeoutMs}
            supplierTls={supplierTlsForm}
            endpointTls={endpointTlsForm}
            canWrite={canWrite}
            isSuperAdmin={isSuperAdmin}
            onUpdateEndpoint={updateActiveEndpoint}
            onChangeSupplierTimeoutMs={setSupplierTimeoutMs}
            onChangeSupplierTls={setSupplierTlsForm}
            onChangeEndpointTls={setEndpointTlsForm}
          />
        )}
      </div>
    </div>
  );
}
