import React from "react";
import { Tooltip, cn } from "@/components/ui";
import { useT } from "@/locales";

export type HexagonStatus = "healthy" | "degraded" | "down" | "unknown";

export interface HexagonNode {
  readonly id: string;
  readonly label: string;
  readonly shortLabel?: string;
  readonly status: HexagonStatus;
  readonly badge?: string;
  readonly tooltipLines?: readonly string[];
}

export interface ServiceHexagonGridProps {
  nodes: readonly HexagonNode[];
  selectedId?: string | null;
  onSelect?: (id: string | null) => void;
  emptyText?: React.ReactNode;
  className?: string;
}

const HEX_CLIP = "polygon(50% 0%, 100% 25%, 100% 75%, 50% 100%, 0% 75%, 0% 25%)";

function shortName(name: string): string {
  return name
    .replace(/^mazbri-/, "")
    .replace(/-service$/, "")
    .replace(/-api$/, "")
    .replace(/-v\d+$/, "");
}

const STATUS_DOT_CLASS: Record<HexagonStatus, string> = {
  healthy: "bg-success-fg",
  degraded: "bg-warning-fg",
  down: "bg-danger-fg",
  unknown: "bg-muted-foreground",
};

const STATUS_BORDER_CLASS: Record<HexagonStatus, string> = {
  healthy: "bg-border",
  degraded: "bg-warning-fg/55",
  down: "bg-danger-fg/65",
  unknown: "bg-muted-foreground/45",
};

const STATUS_BADGE_CLASS: Record<HexagonStatus, string> = {
  healthy: "bg-primary-soft text-primary",
  degraded: "bg-warning-bg text-warning-fg",
  down: "bg-danger-fg/12 text-danger-fg",
  unknown: "bg-muted text-muted-foreground",
};

export function ServiceHexagonGrid({
  nodes,
  selectedId = null,
  onSelect,
  emptyText,
  className,
}: ServiceHexagonGridProps) {
  const { t } = useT();

  if (nodes.length === 0) {
    return (
      <div className="py-6 text-center text-xs text-muted-foreground">
        {emptyText ?? t("ops.services.empty")}
      </div>
    );
  }

  const legendItems: readonly { status: HexagonStatus; label: string }[] = [
    { status: "healthy", label: t("ops.components.statusName.healthy") },
    { status: "degraded", label: t("ops.components.statusName.degraded") },
    { status: "down", label: t("ops.components.statusName.down") },
    { status: "unknown", label: t("ops.components.statusName.unknown") },
  ];

  return (
    <div className={cn("space-y-3", className)}>
      <div className="flex flex-wrap items-center gap-4 text-xs text-muted-foreground">
        {legendItems.map((item) => (
          <span key={item.status} className="inline-flex items-center gap-1.5">
            <span
              className={cn(
                "h-2 w-2 rounded-full shrink-0",
                STATUS_DOT_CLASS[item.status],
              )}
            />
            <span>{item.label}</span>
          </span>
        ))}
      </div>

      <div className="flex flex-wrap gap-2.5 py-1">
        {nodes.map((node) => {
          const active = node.id === selectedId;
          const tooltipContent = node.tooltipLines?.length ? (
            <div className="space-y-0.5 text-[11px] leading-relaxed">
              {node.tooltipLines.map((line, i) => (
                <div key={i}>{line}</div>
              ))}
            </div>
          ) : (
            node.label
          );

          return (
            <Tooltip key={node.id} content={tooltipContent} side="top">
              <button
                type="button"
                onClick={() => onSelect?.(active ? null : node.id)}
                className={cn(
                  "group relative h-[108px] w-[96px] p-0 border-0 bg-transparent cursor-pointer transition-transform duration-150 hover:-translate-y-0.5 focus-visible:outline-none",
                  active && "-translate-y-0.5",
                )}
                aria-pressed={active}
              >
                <div
                  style={{ clipPath: HEX_CLIP }}
                  className={cn(
                    "flex h-full w-full items-center justify-center transition-colors",
                    active
                      ? "bg-primary"
                      : STATUS_BORDER_CLASS[node.status],
                  )}
                >
                  <div
                    style={{ clipPath: HEX_CLIP }}
                    className={cn(
                      "flex h-[calc(100%-3px)] w-[calc(100%-3px)] flex-col items-center justify-center gap-1 px-2 py-4 transition-colors",
                      active
                        ? "bg-primary-soft/60"
                        : "bg-card group-hover:bg-hover-bg/60",
                    )}
                  >
                    <span
                      className={cn(
                        "h-2 w-2 rounded-full shrink-0",
                        STATUS_DOT_CLASS[node.status],
                      )}
                    />
                    <span className="w-full text-center text-xs font-semibold leading-tight text-foreground line-clamp-2 break-words">
                      {node.shortLabel || shortName(node.label)}
                    </span>
                    {node.badge && (
                      <code
                        className={cn(
                          "max-w-[88%] truncate rounded-md px-1.5 py-0.5 text-[10px] font-medium font-mono-tech leading-none",
                          STATUS_BADGE_CLASS[node.status],
                        )}
                      >
                        {node.badge}
                      </code>
                    )}
                  </div>
                </div>
              </button>
            </Tooltip>
          );
        })}
      </div>
    </div>
  );
}
