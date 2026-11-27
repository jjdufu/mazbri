import React, { useMemo, useState } from "react";
import {
  AlertCircle,
  ArrowLeftRight,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Send,
  Zap,
} from "lucide-react";
import type { ConnectorRun, StageResult } from "@/api/upstream-connectors";
import { useT } from "@/locales";
import { Badge, cn } from "@/components/ui";
import {
  extractPipeline,
  prettyJson,
  STAGE_ORDER,
} from "../engine/workbenchHelpers";

export interface WorkbenchPipelineProps {
  run: ConnectorRun | null;
  inbound?: { method?: string; path?: string; body?: unknown };
  className?: string;
}

function packRecord(rec?: Record<string, unknown> | null): string {
  if (!rec) return "";
  const parts: string[] = [];
  if (rec.method || rec.url) {
    parts.push(`${String(rec.method || "")} ${String(rec.url || "")}`.trim());
  }
  if (rec.detail) parts.push(String(rec.detail));
  if (rec.status != null) parts.push(`HTTP ${rec.status}`);
  const hdrs = rec.headers_masked || rec.headers;
  if (hdrs && typeof hdrs === "object" && Object.keys(hdrs as object).length) {
    parts.push(prettyJson(hdrs));
  }
  if (rec.body_preview != null) parts.push(prettyJson(rec.body_preview));
  return parts.join("\n\n");
}

export function WorkbenchPipeline({
  run,
  inbound,
  className,
}: WorkbenchPipelineProps) {
  const { t } = useT();
  const [openStage, setOpenStage] = useState<string>("");

  const pipe = useMemo(() => extractPipeline(run), [run]);
  const stages = useMemo<StageResult[]>(() => {
    const raw = Array.isArray(run?.stages) ? run.stages : [];
    const orderIdx = (name: string) => {
      const idx = (STAGE_ORDER as readonly string[]).indexOf(name);
      return idx >= 0 ? idx : 99;
    };
    return [...raw].sort((a, b) => {
      const oa = orderIdx(a.name);
      const ob = orderIdx(b.name);
      if (oa !== ob) return oa - ob;
      return 0;
    });
  }, [run]);

  const hasData = Boolean(
    pipe.transformedRequest || pipe.upstreamResponse || run,
  );

  const inboundText = useMemo(() => {
    if (!inbound) return "";
    return prettyJson({
      method: inbound.method,
      path: inbound.path,
      body: inbound.body,
    });
  }, [inbound]);

  const transformedText = useMemo(
    () =>
      packRecord(
        pipe.transformedRequest as Record<string, unknown> | undefined,
      ),
    [pipe.transformedRequest],
  );

  const upstreamText = useMemo(() => {
    const rec = pipe.upstreamResponse as Record<string, unknown> | undefined;
    if (rec) return packRecord(rec);
    const tenantRes = pipe.tenantResponse as Record<string, unknown> | undefined;
    if (!tenantRes) return "";
    return packRecord({
      status: tenantRes.status,
      url: tenantRes.url,
      headers: tenantRes.headers,
      body_preview: tenantRes.body ?? tenantRes.body_preview,
    });
  }, [pipe.upstreamResponse, pipe.tenantResponse]);

  const maxStageMs = useMemo(
    () => Math.max(1, ...stages.map((s) => s.latency_ms || 0)),
    [stages],
  );

  const stageLabel = (name: string) => {
    const key = `upstreamConnector.stage.${name}`;
    const translated = t(key);
    return translated === key ? name : translated;
  };

  const stageWidthPct = (ms: number) => {
    if (!ms || ms <= 0) return 4;
    return Math.min(100, Math.max(6, Math.round((ms / maxStageMs) * 100)));
  };

  const activeStageDetail = useMemo(() => {
    if (!openStage) return "";
    const hit = stages.find((s) => s.name === openStage);
    if (!hit) return "";
    return prettyJson({
      ok: hit.ok,
      latency_ms: hit.latency_ms,
      ...(hit.error ? { error: hit.error } : {}),
      ...(hit.request_summary ? { request: hit.request_summary } : {}),
      ...(hit.response_summary ? { response: hit.response_summary } : {}),
    });
  }, [openStage, stages]);

  if (!hasData) {
    return (
      <div className={cn("py-6 text-center text-xs text-muted-foreground", className)}>
        {t("upstreamConnector.pipelineEmpty")}
      </div>
    );
  }

  return (
    <div className={cn("space-y-4", className)}>
      <div className="grid grid-cols-1 xl:grid-cols-3 gap-2">
        <section className="min-w-0 rounded-lg border border-border bg-card overflow-hidden">
          <header className="flex items-center gap-1.5 border-b border-border bg-muted/45 px-2.5 py-1.5 text-xs font-semibold text-foreground">
            <Send className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
            <span className="truncate">{t("upstreamConnector.pipeTenantReq")}</span>
          </header>
          <pre className="m-0 max-h-60 overflow-auto p-2.5 font-mono-tech text-[11px] leading-relaxed text-foreground whitespace-pre-wrap break-all">
            {inboundText || "—"}
          </pre>
        </section>

        <section className="min-w-0 rounded-lg border border-border bg-card overflow-hidden">
          <header className="flex items-center gap-1.5 border-b border-border bg-muted/45 px-2.5 py-1.5 text-xs font-semibold text-foreground">
            <Zap className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
            <span className="truncate">{t("upstreamConnector.pipeUpstreamReq")}</span>
          </header>
          <pre className="m-0 max-h-60 overflow-auto p-2.5 font-mono-tech text-[11px] leading-relaxed text-foreground whitespace-pre-wrap break-all">
            {transformedText || "—"}
          </pre>
        </section>

        <section className="min-w-0 rounded-lg border border-border bg-card overflow-hidden">
          <header className="flex items-center gap-1.5 border-b border-border bg-muted/45 px-2.5 py-1.5 text-xs font-semibold text-foreground">
            <ArrowLeftRight className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
            <span className="truncate">{t("upstreamConnector.pipeUpstreamRes")}</span>
          </header>
          <pre className="m-0 max-h-60 overflow-auto p-2.5 font-mono-tech text-[11px] leading-relaxed text-foreground whitespace-pre-wrap break-all">
            {upstreamText || "—"}
          </pre>
        </section>
      </div>

      {stages.length > 0 && (
        <section className="space-y-2">
          <header className="flex items-center justify-between text-xs font-semibold text-foreground">
            <span>{t("upstreamConnector.timelineTab")}</span>
            <span className="font-mono-tech text-muted-foreground tabular-nums">
              {run?.latency_ms ?? 0} ms
            </span>
          </header>

          <div className="rounded-lg border border-border bg-card divide-y divide-border-subtle overflow-hidden">
            {stages.map((s) => {
              const isOpen = openStage === s.name;
              return (
                <div key={s.name}>
                  <button
                    type="button"
                    onClick={() => setOpenStage(isOpen ? "" : s.name)}
                    className={cn(
                      "w-full grid grid-cols-[minmax(130px,auto)_1fr_auto] items-center gap-3 px-2.5 py-2 text-left text-xs transition-colors hover:bg-hover-bg cursor-pointer",
                      isOpen && "bg-primary-soft/60",
                    )}
                  >
                    <span className="inline-flex items-center gap-1.5 min-w-0">
                      {isOpen ? (
                        <ChevronDown className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
                      ) : (
                        <ChevronRight className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
                      )}
                      {s.ok ? (
                        <CheckCircle2 className="h-3.5 w-3.5 text-success-fg shrink-0" />
                      ) : (
                        <AlertCircle className="h-3.5 w-3.5 text-danger-fg shrink-0" />
                      )}
                      <span className="truncate font-medium text-foreground">
                        {stageLabel(s.name)}
                      </span>
                    </span>

                    <div className="h-2 w-full rounded-full bg-muted overflow-hidden">
                      <div
                        className={cn(
                          "h-full rounded-full transition-all",
                          s.ok ? "bg-success-fg" : "bg-danger-fg",
                        )}
                        style={{ width: `${stageWidthPct(s.latency_ms)}%` }}
                      />
                    </div>

                    <div className="inline-flex items-center gap-2 shrink-0">
                      {s.error && (
                        <Badge variant="danger" className="max-w-[180px] truncate">
                          {s.error}
                        </Badge>
                      )}
                      <span className="font-mono-tech text-[11px] text-muted-foreground tabular-nums w-14 text-right">
                        {s.latency_ms} ms
                      </span>
                    </div>
                  </button>

                  {isOpen && activeStageDetail && (
                    <pre className="m-0 border-t border-border-subtle bg-muted/35 px-3 py-2.5 max-h-64 overflow-auto font-mono-tech text-[11px] leading-relaxed text-foreground whitespace-pre-wrap break-all">
                      {activeStageDetail}
                    </pre>
                  )}
                </div>
              );
            })}
          </div>
        </section>
      )}
    </div>
  );
}
