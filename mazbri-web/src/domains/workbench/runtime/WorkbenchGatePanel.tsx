import React, { useMemo } from "react";
import { AlertTriangle, ShieldCheck } from "lucide-react";
import { useT } from "@/locales";
import { Badge, Button, cn, EmptyState, Switch, Tooltip } from "@/components/ui";
import {
  displayMethodBadge,
  methodTone,
  type GateEpStatus,
} from "../engine/workbenchHelpers";
import {
  selectActiveEp,
  selectCanWriteWorkbench,
  selectGateView,
  selectLeftoverTokens,
  useWorkbenchStore,
} from "../store";

export interface WorkbenchGatePanelProps {
  className?: string;
  onAfterSelect?: () => void;
}

function gateStatusVariant(
  item: GateEpStatus,
): "success" | "warning" | "neutral" | "outline" {
  if (!item.gated) return "neutral";
  if (item.ok) return "success";
  if (
    item.reason === "spec_changed" ||
    item.reason === "unresolved_vars" ||
    item.reason === "spec_invalid"
  ) {
    return "warning";
  }
  return "outline";
}

export function WorkbenchGatePanel({
  className,
  onAfterSelect,
}: WorkbenchGatePanelProps) {
  const { t } = useT();
  const state = useWorkbenchStore();
  const gate = useMemo(() => selectGateView(state), [state]);
  const leftoverTokens = useMemo(() => selectLeftoverTokens(state), [state]);
  const canWrite = useMemo(() => selectCanWriteWorkbench(state), [state]);
  const activeEp = useMemo(() => selectActiveEp(state), [state]);

  const gateReasonText = (item: GateEpStatus): string => {
    if (!item.gated) return t("upstreamConnector.gateOffHint");
    if (item.ok) return t("upstreamConnector.runOk");
    switch (item.reason) {
      case "spec_changed":
        return t("upstreamConnector.gateReasonSpecChanged");
      case "unresolved_vars":
        return t("upstreamConnector.gateReasonUnresolved");
      case "spec_invalid":
        return t("upstreamConnector.gateReasonSpecInvalid");
      default:
        return t("upstreamConnector.gateReasonNotRun");
    }
  };

  const allMet = gate.total > 0 && gate.ok === gate.total;

  return (
    <div
      className={cn(
        "flex flex-col h-full min-h-0 bg-card text-foreground",
        className,
      )}
    >
      <div className="flex h-9 shrink-0 items-center justify-between border-b border-border px-3">
        <span className="inline-flex items-center gap-1.5 text-xs font-semibold text-foreground">
          <ShieldCheck className="h-3.5 w-3.5 text-muted-foreground" />
          {t("upstreamConnector.sidebarGate")}
        </span>
        <Badge
          variant={allMet ? "success" : gate.total > 0 ? "warning" : "neutral"}
          className="font-mono-tech tabular-nums"
        >
          {gate.ok}/{gate.total}
        </Badge>
      </div>

      <div className="flex-1 min-h-0 overflow-auto p-2 space-y-2">
        {leftoverTokens.length > 0 && (
          <div className="rounded-lg border border-warning-border bg-warning-bg p-2.5 space-y-2">
            <div className="flex items-start gap-1.5 text-[11px] leading-snug text-warning-fg">
              <AlertTriangle className="h-3.5 w-3.5 shrink-0 mt-0.5" />
              <span>
                {t("upstreamConnector.staticTokenHint", {
                  headers: leftoverTokens.join(", "),
                })}
              </span>
            </div>
            <Button
              type="button"
              variant="warning"
              size="xs"
              onClick={() => {
                state.setReqTab("auth");
                onAfterSelect?.();
              }}
            >
              {t("upstreamConnector.authTab")}
            </Button>
          </div>
        )}

        {gate.items.length === 0 ? (
          <EmptyState title={t("upstreamConnector.noEndpoints")} />
        ) : (
          <div className="space-y-1">
            {gate.items.map((item, idx) => {
              const isActive = activeEp?.id === item.id;
              return (
                <div
                  key={item.id}
                  role="button"
                  tabIndex={0}
                  onClick={() => {
                    state.switchEndpointByIndex(idx);
                    onAfterSelect?.();
                  }}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      state.switchEndpointByIndex(idx);
                      onAfterSelect?.();
                    }
                  }}
                  className={cn(
                    "flex items-center gap-2 rounded-md border px-2 py-2 text-left transition-colors cursor-pointer",
                    isActive
                      ? "border-primary/45 bg-primary-soft/65"
                      : "border-transparent hover:bg-hover-bg",
                  )}
                >
                  <span
                    className="w-11 shrink-0 font-mono-tech text-[10px] font-bold uppercase tracking-tight"
                    style={{
                      color: `var(--wb-method-${methodTone(item.method)}, var(--foreground))`,
                    }}
                  >
                    {displayMethodBadge(item.method)}
                  </span>

                  <div className="min-w-0 flex-1 space-y-1">
                    <div className="truncate text-xs font-medium text-foreground">
                      {item.name}
                    </div>
                    <Badge
                      variant={gateStatusVariant(item)}
                      className="max-w-full truncate"
                    >
                      {gateReasonText(item)}
                    </Badge>
                  </div>

                  {canWrite && (
                    <Tooltip
                      content={
                        item.gated
                          ? t("upstreamConnector.gateOnHint")
                          : t("upstreamConnector.gateOffHint")
                      }
                      side="left"
                    >
                      <span
                        onClick={(e) => e.stopPropagation()}
                        onKeyDown={(e) => e.stopPropagation()}
                      >
                        <Switch
                          checked={item.gated}
                          onCheckedChange={() => state.toggleEndpointGate(idx)}
                          aria-label={
                            item.gated
                              ? t("upstreamConnector.gateOnHint")
                              : t("upstreamConnector.gateOffHint")
                          }
                        />
                      </span>
                    </Tooltip>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
