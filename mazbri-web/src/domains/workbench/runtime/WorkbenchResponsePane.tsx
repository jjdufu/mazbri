import React, { useMemo, useState } from "react";
import {
  Check,
  Code2,
  Copy,
  Download,
  Keyboard,
  Loader2,
  Search,
  Sparkles,
  X,
} from "lucide-react";
import type { StageResult } from "@/api/upstream-connectors";
import { useT } from "@/locales";
import { copyWithFeedback } from "@/utils/ui/copy-feedback";
import {
  Badge,
  Button,
  cn,
  CodeEditor,
  EmptyState,
  IconButton,
  Tooltip,
  type CodeLanguage,
} from "@/components/ui";
import { extractRetryInfo } from "../engine/authInvalidation";
import { statusGroupClass, statusPhrase } from "../engine/hopp/statusPhrases";
import {
  BYPASS_MGMT_PROBE_PATH,
  byteSizeOf,
  detectResponseBodyKind,
  extractRunResponse,
  formatBytes,
  jsonTreeFits,
  prettyJson,
  tryParseJsonValue,
} from "../engine/workbenchHelpers";
import {
  selectActiveEp,
  selectActiveRuntime,
  useWorkbenchStore,
  type ResponseSubTabId,
} from "../store";
import { WorkbenchJsonTree } from "./WorkbenchJsonTree";
import { WorkbenchPipeline } from "./WorkbenchPipeline";

export interface WorkbenchResponsePaneProps {
  className?: string;
}

function statusBadgeVariant(
  status: number | null | undefined,
  isEngineErr: boolean,
): "neutral" | "xblue" | "success" | "warning" | "danger" {
  if (isEngineErr) return "danger";
  const grp = statusGroupClass(status);
  if (grp === "is-2xx") return "success";
  if (grp === "is-3xx") return "warning";
  if (grp === "is-4xx" || grp === "is-5xx") return "danger";
  if (grp === "is-1xx") return "xblue";
  return "neutral";
}

function formatDiffVal(v: unknown): string {
  if (v === undefined) return "—";
  if (v === null) return "null";
  if (typeof v === "string") return v;
  try {
    return JSON.stringify(v);
  } catch {
    return String(v);
  }
}

export function WorkbenchResponsePane({ className }: WorkbenchResponsePaneProps) {
  const { t } = useT();
  const state = useWorkbenchStore();
  const activeEp = useMemo(() => selectActiveEp(state), [state]);
  const runtime = useMemo(() => selectActiveRuntime(state), [state]);
  const urlMissing = state.urlMissing;
  const setResTab = state.setResTab;
  const setShowShortcuts = state.setShowShortcuts;
  const abortActiveRun = state.abortActiveRun;

  const run = runtime.lastRun;
  const loading = runtime.running;
  const activeTab = runtime.resTab;

  const [headerQuery, setHeaderQuery] = useState("");

  const view = useMemo(() => extractRunResponse(run), [run]);
  const stages = useMemo<StageResult[]>(
    () => (Array.isArray(run?.stages) ? run.stages : []),
    [run],
  );
  const asserts = useMemo(
    () => (Array.isArray(run?.assertions) ? run.assertions : []),
    [run],
  );
  const passedAssertCount = useMemo(
    () => asserts.filter((a) => a.ok).length,
    [asserts],
  );
  const runCapturedEntries = useMemo(
    () => Object.entries(run?.response_preview?.captured || {}),
    [run],
  );
  const retryInfo = useMemo(() => extractRetryInfo(stages), [stages]);

  const requestId = run?.response_preview?.request_id?.trim() || "";
  const upstreamRequestId =
    run?.response_preview?.upstream_request_id?.trim() || "";
  const traceId = run?.response_preview?.trace_id?.trim() || "";
  const hasTraceBar = Boolean(requestId || upstreamRequestId || traceId);

  const engineErrorText = useMemo(() => {
    if (!run) return "";
    if (run.error_code === "ASSERTION_FAILED" && view.status != null) {
      return "";
    }
    const msg = (run.error_message || "").trim();
    const code = (run.error_code || "").trim();
    if (msg && code && !msg.includes(code)) return `${code}: ${msg}`;
    return msg || code;
  }, [run, view.status]);

  const rawBody = useMemo(() => {
    const body = view.body || "";
    if (body.trim()) return body;
    return engineErrorText;
  }, [view.body, engineErrorText]);

  const responseContentType = useMemo(() => {
    const hdrs = view.headers || {};
    for (const [k, v] of Object.entries(hdrs)) {
      if (k.toLowerCase() === "content-type") return v;
    }
    return "";
  }, [view.headers]);

  const bodyKind = useMemo(
    () =>
      detectResponseBodyKind(
        rawBody,
        activeEp?.adapter,
        responseContentType,
      ),
    [rawBody, activeEp?.adapter, responseContentType],
  );

  const parsedJson = useMemo(() => tryParseJsonValue(rawBody), [rawBody]);
  const jsonTooLarge = useMemo(
    () =>
      bodyKind === "json" &&
      parsedJson.ok &&
      !jsonTreeFits(parsedJson.value),
    [bodyKind, parsedJson],
  );

  const displayBody = useMemo(
    () => (bodyKind === "json" ? prettyJson(rawBody) : rawBody),
    [bodyKind, rawBody],
  );

  const editorLang: CodeLanguage =
    bodyKind === "xml" ? "xml" : bodyKind === "json" ? "json" : "text";
  const rawLangLabel =
    bodyKind === "xml" ? "XML" : bodyKind === "json" ? "JSON" : "TEXT";
  const downloadName =
    bodyKind === "xml"
      ? "response.xml"
      : bodyKind === "json"
        ? "response.json"
        : "response.txt";

  const bodyBytes = useMemo(() => byteSizeOf(rawBody), [rawBody]);

  const headerEntries = useMemo(
    () => Object.entries(view.headers || {}),
    [view.headers],
  );
  const filteredHeaderEntries = useMemo(() => {
    const q = headerQuery.trim().toLowerCase();
    if (!q) return headerEntries;
    return headerEntries.filter(
      ([k, v]) => k.toLowerCase().includes(q) || v.toLowerCase().includes(q),
    );
  }, [headerEntries, headerQuery]);

  const isEngineErr = Boolean(
    run?.error_code &&
      run.error_code !== "ASSERTION_FAILED" &&
      view.status == null,
  );

  const statusText = useMemo(() => {
    if (view.status == null) return "—";
    const phrase = statusPhrase(view.status);
    return phrase ? `${view.status} ${phrase}` : String(view.status);
  }, [view.status]);

  const timeLabel =
    run?.probe_path === BYPASS_MGMT_PROBE_PATH
      ? t("upstreamConnector.response.upstreamTime")
      : t("upstreamConnector.response.time");

  const modeBadgeText = useMemo(() => {
    const mode = run?.mode || "";
    if (!mode) return "";
    if (mode === "live" && run?.probe_path === BYPASS_MGMT_PROBE_PATH) {
      return t("upstreamConnector.modeBadge.business");
    }
    const key = `upstreamConnector.modeBadge.${mode}`;
    const translated = t(key);
    return translated === key ? mode : translated;
  }, [run?.mode, run?.probe_path, t]);

  const subTabs = useMemo(() => {
    const assertTotal = asserts.length + runCapturedEntries.length;
    const assertFailed = asserts.some((a) => !a.ok);
    return [
      {
        id: "json" as ResponseSubTabId,
        label: t("upstreamConnector.resJsonTreeTab"),
        badge: "",
        tone: "neutral" as const,
      },
      {
        id: "raw" as ResponseSubTabId,
        label: t("upstreamConnector.resRawTab"),
        badge: "",
        tone: "neutral" as const,
      },
      {
        id: "headers" as ResponseSubTabId,
        label: t("upstreamConnector.resHeadersTab"),
        badge: headerEntries.length ? String(headerEntries.length) : "",
        tone: "neutral" as const,
      },
      {
        id: "assertions" as ResponseSubTabId,
        label: t("upstreamConnector.resAssertCaptureTab"),
        badge: assertTotal
          ? asserts.length
            ? `${passedAssertCount}/${asserts.length}`
            : String(runCapturedEntries.length)
          : "",
        tone: assertFailed
          ? ("danger" as const)
          : asserts.length
            ? ("success" as const)
            : ("neutral" as const),
      },
      {
        id: "pipeline" as ResponseSubTabId,
        label: t("upstreamConnector.resPipelineTab"),
        badge: stages.length ? String(stages.length) : "",
        tone: "neutral" as const,
      },
    ];
  }, [
    asserts,
    headerEntries.length,
    passedAssertCount,
    runCapturedEntries.length,
    stages.length,
    t,
  ]);

  const handleDownload = () => {
    if (!displayBody) return;
    const blob = new Blob([displayBody], { type: "application/octet-stream" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = downloadName;
    a.click();
    URL.revokeObjectURL(url);
  };

  const inbound = activeEp
    ? { method: activeEp.method, path: activeEp.path, body: activeEp.body }
    : undefined;

  return (
    <div
      className={cn(
        "flex flex-col h-full min-h-0 bg-card text-foreground",
        className,
      )}
    >
      {loading ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
          <Loader2 className="h-5 w-5 animate-spin text-primary" />
          <p className="text-xs font-medium text-muted-foreground">
            {t("upstreamConnector.requesting")}
          </p>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => abortActiveRun()}
          >
            <X className="h-3.5 w-3.5" />
            {t("upstreamConnector.sendCancel")}
          </Button>
        </div>
      ) : !run ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center">
          <Code2 className="h-6 w-6 text-muted-foreground/70" />
          <p
            className={cn(
              "text-xs font-medium",
              urlMissing ? "text-danger-fg" : "text-foreground",
            )}
          >
            {urlMissing
              ? t("upstreamConnector.urlMissingSend")
              : t("upstreamConnector.responseIdle")}
          </p>
          <p className="text-[11px] text-muted-foreground">
            {t("upstreamConnector.responseIdleHint")}
          </p>
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="mt-1"
            onClick={() => setShowShortcuts(true)}
          >
            <Keyboard className="h-3.5 w-3.5" />
            {t("upstreamConnector.shortcutsAll")}
          </Button>
        </div>
      ) : (
        <>
          <div className="shrink-0 border-b border-border">
            <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5 px-3 py-2 text-xs">
              <div className="inline-flex items-center gap-1.5">
                <span className="text-muted-foreground">
                  {t("upstreamConnector.response.status")}:
                </span>
                <Badge variant={statusBadgeVariant(view.status, isEngineErr)}>
                  <span className="font-mono-tech tabular-nums">{statusText}</span>
                </Badge>
                {run.error_code && (
                  <Badge variant="danger" className="font-mono-tech">
                    {run.error_code}
                  </Badge>
                )}
              </div>

              <div className="inline-flex items-center gap-1">
                <span className="text-muted-foreground">{timeLabel}:</span>
                <span className="font-mono-tech font-semibold tabular-nums text-foreground">
                  {run.latency_ms == null ? "—" : `${run.latency_ms} ms`}
                </span>
              </div>

              <div className="inline-flex items-center gap-1">
                <span className="text-muted-foreground">
                  {t("upstreamConnector.response.size")}:
                </span>
                <span className="font-mono-tech font-semibold tabular-nums text-foreground">
                  {formatBytes(bodyBytes)}
                </span>
              </div>

              {retryInfo && (
                <Tooltip
                  content={`${t("upstreamConnector.probeRetryReasons")}: ${retryInfo.reasons.join(", ")}`}
                  side="bottom"
                >
                  <Badge variant="warning" className="font-mono-tech tabular-nums">
                    {t("upstreamConnector.probeInternalRetries")}: {retryInfo.count}
                  </Badge>
                </Tooltip>
              )}

              <div className="ml-auto inline-flex items-center gap-1.5">
                {modeBadgeText && <Badge variant="outline">{modeBadgeText}</Badge>}
                <Tooltip content={t("common.actions.copy")} side="bottom">
                  <IconButton
                    size="sm"
                    disabled={!displayBody}
                    aria-label={t("common.actions.copy")}
                    onClick={() => void copyWithFeedback(displayBody)}
                  >
                    <Copy className="h-3.5 w-3.5" />
                  </IconButton>
                </Tooltip>
                <Tooltip content={t("upstreamConnector.downloadBody")} side="bottom">
                  <IconButton
                    size="sm"
                    disabled={!displayBody}
                    aria-label={t("upstreamConnector.downloadBody")}
                    onClick={handleDownload}
                  >
                    <Download className="h-3.5 w-3.5" />
                  </IconButton>
                </Tooltip>
              </div>
            </div>

            {hasTraceBar && (
              <div className="flex flex-wrap items-center gap-1.5 border-t border-border-subtle px-3 py-1.5 bg-muted/25">
                {requestId && (
                  <button
                    type="button"
                    onClick={() => void copyWithFeedback(requestId)}
                    title={t("common.actions.copy")}
                    className="inline-flex h-6 items-center gap-1.5 rounded-md border border-border bg-card px-2 text-[11px] text-muted-foreground hover:bg-hover-bg hover:text-foreground cursor-pointer transition-colors"
                  >
                    <span>{t("upstreamConnector.traceRequestId")}</span>
                    <code className="max-w-[180px] truncate font-mono-tech text-foreground">
                      {requestId}
                    </code>
                    <Copy className="h-3 w-3 shrink-0" />
                  </button>
                )}
                {upstreamRequestId && (
                  <button
                    type="button"
                    onClick={() => void copyWithFeedback(upstreamRequestId)}
                    title={t("common.actions.copy")}
                    className="inline-flex h-6 items-center gap-1.5 rounded-md border border-border bg-card px-2 text-[11px] text-muted-foreground hover:bg-hover-bg hover:text-foreground cursor-pointer transition-colors"
                  >
                    <span>{t("upstreamConnector.traceUpstreamId")}</span>
                    <code className="max-w-[180px] truncate font-mono-tech text-foreground">
                      {upstreamRequestId}
                    </code>
                    <Copy className="h-3 w-3 shrink-0" />
                  </button>
                )}
                {traceId && (
                  <button
                    type="button"
                    onClick={() => void copyWithFeedback(traceId)}
                    title={t("common.actions.copy")}
                    className="inline-flex h-6 items-center gap-1.5 rounded-md border border-border bg-card px-2 text-[11px] text-muted-foreground hover:bg-hover-bg hover:text-foreground cursor-pointer transition-colors"
                  >
                    <span>{t("upstreamConnector.traceIdLabel")}</span>
                    <code className="max-w-[180px] truncate font-mono-tech text-foreground">
                      {traceId}
                    </code>
                    <Copy className="h-3 w-3 shrink-0" />
                  </button>
                )}
              </div>
            )}
          </div>

          <div
            role="tablist"
            className="flex shrink-0 items-stretch gap-1 border-b border-border bg-muted/35 px-2 overflow-x-auto"
          >
            {subTabs.map((tab) => {
              const active = activeTab === tab.id;
              return (
                <button
                  key={tab.id}
                  type="button"
                  role="tab"
                  aria-selected={active}
                  onClick={() => setResTab(tab.id)}
                  className={cn(
                    "relative inline-flex h-8 shrink-0 items-center gap-1.5 rounded-t-md px-2.5 text-xs font-medium transition-colors cursor-pointer",
                    active
                      ? "text-primary font-semibold"
                      : "text-muted-foreground hover:text-foreground hover:bg-hover-bg/60",
                  )}
                >
                  <span>{tab.label}</span>
                  {tab.badge && (
                    <span
                      className={cn(
                        "rounded-full border px-1.5 py-0 text-[10px] font-mono-tech tabular-nums leading-4",
                        tab.tone === "success" &&
                          "border-success-fg/30 bg-success-fg/10 text-success-fg",
                        tab.tone === "danger" &&
                          "border-danger-fg/30 bg-danger-fg/10 text-danger-fg",
                        tab.tone === "neutral" &&
                          "border-border bg-card text-muted-foreground",
                      )}
                    >
                      {tab.badge}
                    </span>
                  )}
                  {active && (
                    <span className="absolute inset-x-1.5 bottom-0 h-0.5 rounded-full bg-primary" />
                  )}
                </button>
              );
            })}
          </div>

          <div className="flex-1 min-h-0 flex flex-col overflow-hidden">
            {activeTab === "json" && (
              <div className="flex-1 min-h-0 flex flex-col gap-2 p-2.5 overflow-hidden">
                {runCapturedEntries.length > 0 && (
                  <div className="flex flex-wrap items-center gap-1.5 rounded-lg border border-success-fg/25 bg-success-fg/8 px-2.5 py-1.5 shrink-0">
                    <span className="inline-flex items-center gap-1 text-[11px] font-medium text-success-fg">
                      <Sparkles className="h-3 w-3" />
                      {t("upstreamConnector.capturesTab")}
                    </span>
                    {runCapturedEntries.map(([k, v]) => (
                      <button
                        key={k}
                        type="button"
                        onClick={() => void copyWithFeedback(`{{captures.${k}}}`)}
                        title={t("common.actions.copy")}
                        className="inline-flex h-6 items-center gap-1 rounded-md border border-success-fg/25 bg-card px-2 font-mono-tech text-[11px] text-foreground hover:bg-hover-bg cursor-pointer"
                      >
                        <span className="text-success-fg">{`{{captures.${k}}}`}</span>
                        <span className="text-muted-foreground">=</span>
                        <span className="max-w-[160px] truncate">{v}</span>
                      </button>
                    ))}
                  </div>
                )}

                {jsonTooLarge && (
                  <p className="text-[11px] text-muted-foreground shrink-0">
                    {t("upstreamConnector.jsonTooLarge")}
                  </p>
                )}

                {bodyKind === "json" && parsedJson.ok && !jsonTooLarge ? (
                  <WorkbenchJsonTree
                    value={parsedJson.value}
                    className="flex-1 min-h-0"
                  />
                ) : (
                  <div className="flex-1 min-h-0 overflow-auto">
                    <CodeEditor
                      value={displayBody}
                      readOnly
                      language={editorLang}
                      minHeight={200}
                    />
                  </div>
                )}
              </div>
            )}

            {activeTab === "raw" && (
              <div className="flex-1 min-h-0 flex flex-col gap-2 p-2.5 overflow-hidden">
                <div className="flex items-center justify-between shrink-0">
                  <Badge variant="outline" className="font-mono-tech">
                    {rawLangLabel}
                  </Badge>
                  <Button
                    type="button"
                    variant="ghost"
                    size="xs"
                    disabled={!displayBody}
                    onClick={() => void copyWithFeedback(displayBody)}
                  >
                    <Copy className="h-3 w-3" />
                    {t("common.actions.copy")}
                  </Button>
                </div>
                <div className="flex-1 min-h-0 overflow-auto">
                  <CodeEditor
                    value={displayBody}
                    readOnly
                    language={editorLang}
                    minHeight={220}
                  />
                </div>
              </div>
            )}

            {activeTab === "headers" && (
              <div className="flex-1 min-h-0 flex flex-col p-2.5 gap-2 overflow-hidden">
                {headerEntries.length > 0 && (
                  <div className="flex items-center gap-1.5 h-7 rounded-md border border-input bg-card px-2 shrink-0">
                    <Search className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
                    <input
                      type="search"
                      value={headerQuery}
                      onChange={(e) => setHeaderQuery(e.target.value)}
                      placeholder={t("common.fields.search")}
                      aria-label={t("common.fields.search")}
                      className="flex-1 min-w-0 bg-transparent text-xs text-foreground placeholder:text-muted-foreground focus:outline-none"
                    />
                    {headerQuery && (
                      <button
                        type="button"
                        onClick={() => setHeaderQuery("")}
                        aria-label={t("upstreamConnector.clearKv")}
                        className="inline-flex h-5 w-5 items-center justify-center rounded-md text-muted-foreground hover:bg-hover-bg hover:text-foreground cursor-pointer"
                      >
                        <X className="h-3 w-3" />
                      </button>
                    )}
                  </div>
                )}

                <div className="flex-1 min-h-0 overflow-auto">
                  {filteredHeaderEntries.length === 0 ? (
                    <EmptyState title={t("common.feedback.noData")} />
                  ) : (
                    <div className="rounded-lg border border-border overflow-hidden divide-y divide-border-subtle">
                      {filteredHeaderEntries.map(([k, v]) => (
                        <div
                          key={k}
                          role="button"
                          tabIndex={0}
                          onClick={() => void copyWithFeedback(v)}
                          onKeyDown={(e) => {
                            if (e.key === "Enter" || e.key === " ") {
                              e.preventDefault();
                              void copyWithFeedback(v);
                            }
                          }}
                          className="group grid grid-cols-[minmax(140px,0.7fr)_minmax(0,1.5fr)_auto] items-start gap-2 px-3 py-1.5 text-xs font-mono-tech hover:bg-hover-bg cursor-pointer transition-colors"
                        >
                          <span className="text-muted-foreground break-all">{k}</span>
                          <span className="text-foreground break-all">{v}</span>
                          <Copy className="h-3.5 w-3.5 text-muted-foreground opacity-0 group-hover:opacity-100 mt-0.5 shrink-0" />
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              </div>
            )}

            {activeTab === "assertions" && (
              <div className="flex-1 min-h-0 overflow-auto p-3 space-y-4">
                <section className="space-y-2">
                  <header className="flex items-center justify-between text-xs font-semibold text-foreground">
                    <span>{t("upstreamConnector.assertions")}</span>
                    <span className="font-mono-tech text-muted-foreground tabular-nums">
                      {passedAssertCount}/{asserts.length}
                    </span>
                  </header>

                  {asserts.length === 0 ? (
                    <p className="text-xs text-muted-foreground">
                      {t("upstreamConnector.noAssertions")}
                    </p>
                  ) : (
                    <div className="space-y-2">
                      {asserts.map((a, i) => (
                        <div
                          key={`${a.name}-${i}`}
                          className={cn(
                            "rounded-lg border border-border border-l-3 bg-card p-2.5 space-y-1.5",
                            a.ok
                              ? "border-l-success-fg"
                              : "border-l-danger-fg",
                          )}
                        >
                          <div className="flex flex-wrap items-center gap-2 text-xs">
                            <Badge variant={a.ok ? "success" : "danger"}>
                              {a.ok ? (
                                <Check className="h-3 w-3" />
                              ) : (
                                <X className="h-3 w-3" />
                              )}
                            </Badge>
                            <span className="font-medium text-foreground">
                              {a.name}
                            </span>
                            {a.message && (
                              <span className="text-muted-foreground">
                                {a.message}
                              </span>
                            )}
                          </div>
                          {(a.expected !== undefined || a.actual !== undefined) && (
                            <div className="grid grid-cols-1 sm:grid-cols-2 gap-2 pt-1">
                              <div className="rounded-md bg-muted/50 px-2 py-1 text-[11px]">
                                <span className="text-muted-foreground mr-1.5">
                                  {t("upstreamConnector.assertExpected")}:
                                </span>
                                <code className="font-mono-tech text-foreground break-all">
                                  {formatDiffVal(a.expected)}
                                </code>
                              </div>
                              <div className="rounded-md bg-muted/50 px-2 py-1 text-[11px]">
                                <span className="text-muted-foreground mr-1.5">
                                  {t("upstreamConnector.assertActual")}:
                                </span>
                                <code className="font-mono-tech text-foreground break-all">
                                  {formatDiffVal(a.actual)}
                                </code>
                              </div>
                            </div>
                          )}
                        </div>
                      ))}
                    </div>
                  )}
                </section>

                <section className="space-y-2">
                  <header className="flex items-center justify-between text-xs font-semibold text-foreground">
                    <span>{t("upstreamConnector.capturesTab")}</span>
                    <span className="font-mono-tech text-muted-foreground tabular-nums">
                      {runCapturedEntries.length}
                    </span>
                  </header>

                  {runCapturedEntries.length === 0 ? (
                    <p className="text-xs text-muted-foreground">
                      {t("upstreamConnector.capturedEmpty")}
                    </p>
                  ) : (
                    <div className="rounded-lg border border-border overflow-hidden divide-y divide-border-subtle">
                      {runCapturedEntries.map(([k, v]) => (
                        <div
                          key={k}
                          role="button"
                          tabIndex={0}
                          onClick={() =>
                            void copyWithFeedback(`{{captures.${k}}}`)
                          }
                          onKeyDown={(e) => {
                            if (e.key === "Enter" || e.key === " ") {
                              e.preventDefault();
                              void copyWithFeedback(`{{captures.${k}}}`);
                            }
                          }}
                          className="group grid grid-cols-[minmax(140px,0.7fr)_minmax(0,1.5fr)_auto] items-start gap-2 px-3 py-1.5 text-xs font-mono-tech hover:bg-hover-bg cursor-pointer transition-colors"
                        >
                          <span className="text-success-fg break-all">
                            {`{{captures.${k}}}`}
                          </span>
                          <span className="text-foreground break-all">{v}</span>
                          <Copy className="h-3.5 w-3.5 text-muted-foreground opacity-0 group-hover:opacity-100 mt-0.5 shrink-0" />
                        </div>
                      ))}
                    </div>
                  )}
                </section>
              </div>
            )}

            {activeTab === "pipeline" && (
              <div className="flex-1 min-h-0 overflow-auto p-3">
                <WorkbenchPipeline run={run} inbound={inbound} />
              </div>
            )}
          </div>
        </>
      )}
    </div>
  );
}
