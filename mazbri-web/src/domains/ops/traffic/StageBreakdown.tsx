import React, { useEffect, useMemo, useState } from "react";
import { Loader2 } from "lucide-react";
import { Badge, Button, CopyableText, Tooltip, cn } from "@/components/ui";
import { fetchCallStages } from "@/api/call-monitor";
import { useT } from "@/locales";
import type {
  CallStageAccessRow,
  CallStageEntry,
} from "@/types/api/call-monitor";
import { canDrillToTrace, isTraceExpired } from "@/utils/ops/trace-link";
import { useOpsRole } from "../shared/opsHelpers";
import { computeStageBreakdown, formatStageShare } from "./stage-breakdown";

export interface StageBreakdownProps {
  requestId?: string;
  traceId?: string;
  timestamp?: string | number | null;
  totalDurationMs?: number;
  stages?: CallStageEntry[] | null;
  calls?: CallStageAccessRow[] | null;
  onOpenTrace?: (traceId: string) => void;
}

export function StageBreakdown({
  requestId = "",
  traceId = "",
  timestamp = null,
  totalDurationMs,
  stages: preloadedStages = null,
  calls: preloadedCalls = null,
  onOpenTrace,
}: StageBreakdownProps) {
  const { t } = useT();
  const { isAdmin } = useOpsRole();
  const [loading, setLoading] = useState(false);
  const [errorText, setErrorText] = useState("");
  const [loadedStages, setLoadedStages] = useState<CallStageEntry[]>([]);
  const [loadedCalls, setLoadedCalls] = useState<CallStageAccessRow[]>([]);

  useEffect(() => {
    if (preloadedStages || !requestId) return;
    const ctrl = new AbortController();
    setLoading(true);
    setErrorText("");
    fetchCallStages({ request_id: requestId }, ctrl.signal)
      .then((res) => {
        if (ctrl.signal.aborted) return;
        setLoadedStages(res.stages ?? []);
        setLoadedCalls(res.calls ?? []);
      })
      .catch(() => {
        if (ctrl.signal.aborted) return;
        setErrorText(t("ops.calls.stages.loadFailed"));
      })
      .finally(() => {
        if (!ctrl.signal.aborted) setLoading(false);
      });
    return () => ctrl.abort();
  }, [preloadedStages, requestId, t]);

  const rows = preloadedStages ?? loadedStages;
  const callRows = preloadedCalls ?? loadedCalls;

  const model = useMemo(
    () =>
      computeStageBreakdown(rows, {
        calls: callRows,
        totalDurationMs,
      }),
    [rows, callRows, totalDurationMs],
  );

  const expired = isTraceExpired(timestamp);
  const canDrill = canDrillToTrace(timestamp, isAdmin);

  return (
    <div className="min-h-[44px] space-y-2 px-3 py-2 text-xs">
      {loading ? (
        <div className="flex items-center gap-2 text-muted-foreground">
          <Loader2 className="h-3.5 w-3.5 animate-spin text-primary" />
          <span>{t("layout.loading")}</span>
        </div>
      ) : errorText ? (
        <div className="text-danger-fg">{errorText}</div>
      ) : !model.summaryBar && model.subBars.length === 0 ? (
        <div className="text-muted-foreground">{t("ops.calls.stages.empty")}</div>
      ) : (
        <div className="space-y-1.5">
          {model.summaryBar && (
            <div className="grid grid-cols-1 gap-2 border-b border-border-subtle pb-2 sm:grid-cols-[minmax(200px,260px)_1fr_110px] sm:items-center">
              <div className="flex items-center gap-1.5 min-w-0">
                <span
                  className="truncate font-semibold text-foreground"
                  title={t("ops.calls.stages.gatewaySummary")}
                >
                  {t("ops.calls.stages.gatewaySummary")}
                </span>
                {model.summaryBar.statusCode > 0 && (
                  <Badge variant={model.summaryBar.error ? "danger" : "success"}>
                    {model.summaryBar.statusCode}
                  </Badge>
                )}
                {model.summaryBar.errorCode && (
                  <Badge variant="danger">{model.summaryBar.errorCode}</Badge>
                )}
              </div>
              <div className="h-2.5 w-full overflow-hidden rounded-md bg-muted">
                <div
                  className={cn(
                    "h-full rounded-md",
                    model.summaryBar.error
                      ? "bg-danger-fg"
                      : "bg-muted-foreground/50",
                  )}
                  style={{ width: "100%" }}
                />
              </div>
              <span className="text-right font-mono-tech font-semibold tabular-nums text-foreground">
                {model.summaryBar.ms} ms
              </span>
            </div>
          )}

          {model.unsampledSubStages && (
            <div className="text-muted-foreground">
              {t("ops.calls.stages.unsampledHint")}
            </div>
          )}

          {model.subBars.map((bar) => (
            <div
              key={bar.key}
              className="grid grid-cols-1 gap-2 sm:grid-cols-[minmax(200px,260px)_1fr_110px] sm:items-center"
            >
              <div className="flex items-center gap-1.5 min-w-0">
                <span
                  className="truncate font-mono-tech text-foreground"
                  title={bar.label}
                >
                  {bar.label}
                </span>
                {bar.statusCode > 0 && (
                  <Badge variant={bar.error ? "danger" : "success"}>
                    {bar.statusCode}
                  </Badge>
                )}
                {bar.errorCode && <Badge variant="danger">{bar.errorCode}</Badge>}
              </div>
              <div className="h-2.5 w-full overflow-hidden rounded-md bg-muted">
                <div
                  className={cn(
                    "h-full rounded-md",
                    bar.error ? "bg-danger-fg" : "bg-primary",
                  )}
                  style={{ width: `${bar.percent}%` }}
                />
              </div>
              <span className="text-right font-mono-tech tabular-nums text-muted-foreground">
                {bar.ms} ms ({formatStageShare(bar.share)})
              </span>
            </div>
          ))}
        </div>
      )}

      {traceId && (
        <div className="flex flex-wrap items-center gap-2 border-t border-border-subtle pt-2">
          {canDrill && onOpenTrace ? (
            <>
              <Button
                type="button"
                variant="link"
                size="xs"
                onClick={() => onOpenTrace(traceId)}
              >
                {t("ops.calls.stages.openTrace")}
              </Button>
              <CopyableText value={traceId} mono truncate maxWidthClass="max-w-[240px]" />
            </>
          ) : isAdmin && expired ? (
            <Tooltip content={t("ops.traces.expiredTooltip")}>
              <span className="inline-flex items-center text-muted-foreground opacity-70">
                <CopyableText value={traceId} mono truncate maxWidthClass="max-w-[240px]" />
              </span>
            </Tooltip>
          ) : (
            <CopyableText value={traceId} mono truncate maxWidthClass="max-w-[240px]" />
          )}
        </div>
      )}
    </div>
  );
}
