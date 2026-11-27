import React, { useMemo, useState } from "react";
import { History, Search, X } from "lucide-react";
import type { ConnectorRun } from "@/api/upstream-connectors";
import { useT } from "@/locales";
import { Badge, cn, EmptyState } from "@/components/ui";
import {
  BYPASS_MGMT_PROBE_PATH,
  formatTime,
} from "../engine/workbenchHelpers";
import { selectActiveRuntime, useWorkbenchStore } from "../store";

export interface WorkbenchHistoryPanelProps {
  className?: string;
  onAfterSelect?: () => void;
}

export function WorkbenchHistoryPanel({
  className,
  onAfterSelect,
}: WorkbenchHistoryPanelProps) {
  const { t } = useT();
  const state = useWorkbenchStore();
  const runs = state.runs;
  const activeRunId = useMemo(
    () => selectActiveRuntime(state).lastRun?.id ?? null,
    [state],
  );

  const [query, setQuery] = useState("");

  const filteredRuns = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return runs;
    return runs.filter((r) => {
      const blob = [
        r.endpoint_key || "",
        r.mode || "",
        String(r.response_preview?.status ?? ""),
        r.error_code || "",
      ]
        .join(" ")
        .toLowerCase();
      return blob.includes(q);
    });
  }, [runs, query]);

  const runModeText = (r: ConnectorRun): string => {
    if (r.mode === "live" && r.probe_path === BYPASS_MGMT_PROBE_PATH) {
      return t("upstreamConnector.modeBadge.business");
    }
    const key = `upstreamConnector.modeBadge.${r.mode}`;
    const translated = t(key);
    return translated === key ? r.mode : translated;
  };

  return (
    <div
      className={cn(
        "flex flex-col h-full min-h-0 bg-card text-foreground",
        className,
      )}
    >
      <div className="flex h-9 shrink-0 items-center justify-between border-b border-border px-3">
        <span className="inline-flex items-center gap-1.5 text-xs font-semibold text-foreground">
          <History className="h-3.5 w-3.5 text-muted-foreground" />
          {t("upstreamConnector.sidebarHistory")}
        </span>
        <span className="font-mono-tech text-[11px] text-muted-foreground tabular-nums">
          {runs.length}
        </span>
      </div>

      <div className="flex h-8 shrink-0 items-center gap-1.5 border-b border-border bg-muted/35 px-2.5">
        <Search className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={t("upstreamConnector.filterHistoryPh")}
          aria-label={t("upstreamConnector.filterHistoryPh")}
          className="flex-1 min-w-0 bg-transparent text-xs text-foreground placeholder:text-muted-foreground focus:outline-none"
        />
        {query && (
          <button
            type="button"
            onClick={() => setQuery("")}
            aria-label={t("upstreamConnector.clearKv")}
            className="inline-flex h-5 w-5 items-center justify-center rounded-md text-muted-foreground hover:bg-hover-bg hover:text-foreground cursor-pointer"
          >
            <X className="h-3 w-3" />
          </button>
        )}
      </div>

      <div className="flex-1 min-h-0 overflow-auto p-1.5 space-y-1">
        {filteredRuns.length === 0 ? (
          <EmptyState title={t("common.feedback.noData")} className="m-1" />
        ) : (
          filteredRuns.map((r) => {
            const active = r.id === activeRunId;
            const statusCode = r.response_preview?.status;
            return (
              <button
                key={r.id}
                type="button"
                onClick={() => {
                  state.selectHistoryRun(r);
                  onAfterSelect?.();
                }}
                className={cn(
                  "w-full flex items-start gap-2 rounded-md border px-2 py-2 text-left transition-colors cursor-pointer",
                  active
                    ? "border-primary/45 bg-primary-soft/65"
                    : "border-transparent hover:bg-hover-bg",
                )}
              >
                <span
                  className={cn(
                    "mt-1.5 h-2 w-2 rounded-full shrink-0",
                    r.ok ? "bg-success-fg" : "bg-danger-fg",
                  )}
                />
                <div className="min-w-0 flex-1 space-y-1">
                  <div className="flex items-center justify-between gap-2">
                    <span className="truncate text-xs font-medium text-foreground">
                      {r.endpoint_key || "—"}
                    </span>
                    <span className="font-mono-tech text-[11px] text-muted-foreground tabular-nums shrink-0">
                      {r.latency_ms ?? 0}ms
                    </span>
                  </div>
                  <div className="flex flex-wrap items-center gap-1.5 text-[11px] text-muted-foreground">
                    <span>{runModeText(r)}</span>
                    {statusCode != null && (
                      <Badge
                        variant={
                          statusCode >= 200 && statusCode < 300
                            ? "success"
                            : statusCode >= 400
                              ? "danger"
                              : "warning"
                        }
                        className="px-1.5 py-0 text-[10px] font-mono-tech tabular-nums"
                      >
                        {statusCode}
                      </Badge>
                    )}
                    <span className="ml-auto font-mono-tech text-[10px] tabular-nums">
                      {formatTime(r.created_at)}
                    </span>
                  </div>
                </div>
              </button>
            );
          })
        )}
      </div>
    </div>
  );
}
