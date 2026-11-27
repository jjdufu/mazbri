import type React from "react";
import { Loader2, Plus, X } from "lucide-react";
import { cn } from "@/components/ui";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import { canWriteUpstreamConnector } from "@/utils/ops/roles";
import type { ConnectorEndpoint } from "@/api/upstream-connectors";
import { displayMethodBadge } from "../engine/workbenchHelpers";
import {
  selectGateView,
  selectOpenTabs,
  useWorkbenchStore,
} from "../store";
import { methodTextClasses } from "./methodStyles";

function shortAdapterBadge(adapter: string): string {
  switch (adapter) {
    case "http_envelope":
      return "ENV";
    case "sign_form":
      return "SIGN";
    case "jsonrpc":
      return "RPC";
    case "graphql":
      return "GQL";
    case "grpc":
      return "gRPC";
    case "soap_xml":
      return "SOAP";
    case "ws_unary":
      return "WS";
    case "job_poll":
      return "JOB";
    default:
      return adapter.toUpperCase();
  }
}

export function WorkbenchTabStrip() {
  const { t } = useT();
  const role = useAuthStore((s) => s.info.role);
  const canWrite = canWriteUpstreamConnector(role);

  const tabs = useWorkbenchStore(selectOpenTabs);
  const activeTabId = useWorkbenchStore((s) => s.activeTabId);
  const gate = useWorkbenchStore(selectGateView);
  const openEndpointTab = useWorkbenchStore((s) => s.openEndpointTab);
  const closeEndpointTab = useWorkbenchStore((s) => s.closeEndpointTab);
  const addEndpoint = useWorkbenchStore((s) => s.addEndpoint);
  const duplicateEndpoint = useWorkbenchStore((s) => s.duplicateEndpoint);

  const gateDotClass = (ep: ConnectorEndpoint) => {
    if (ep.gate === false) {
      return "bg-transparent border border-muted-foreground/60";
    }
    const hit = gate.items.find((g) => g.id === ep.id || g.name === ep.name);
    return hit?.ok ? "bg-success-fg" : "bg-warning-fg";
  };

  const handleTabKeyDown = (
    ev: React.KeyboardEvent<HTMLDivElement>,
    idx: number,
  ) => {
    if (tabs.length === 0) return;
    if (ev.key === "ArrowRight") {
      ev.preventDefault();
      const next = tabs[(idx + 1) % tabs.length];
      if (next) openEndpointTab(next.id);
    } else if (ev.key === "ArrowLeft") {
      ev.preventDefault();
      const prev = tabs[(idx - 1 + tabs.length) % tabs.length];
      if (prev) openEndpointTab(prev.id);
    } else if (ev.key === "Enter" || ev.key === " ") {
      ev.preventDefault();
      const cur = tabs[idx];
      if (cur) openEndpointTab(cur.id);
    }
  };

  return (
    <div
      role="tablist"
      className="flex h-9 shrink-0 items-stretch border-b border-border bg-muted/40"
    >
      <div className="flex min-w-0 flex-1 items-stretch overflow-x-auto">
        {tabs.map((item, idx) => {
          const isActive = item.id === activeTabId;
          return (
            <div
              key={item.id}
              role="tab"
              tabIndex={0}
              aria-selected={isActive}
              onClick={() => openEndpointTab(item.id)}
              onDoubleClick={() => {
                if (canWrite) duplicateEndpoint(item.index);
              }}
              onKeyDown={(e) => handleTabKeyDown(e, idx)}
              data-testid={`wb-tab-${item.id}`}
              className={cn(
                "group relative inline-flex max-w-[220px] shrink-0 items-center gap-1.5 border-r border-border px-2.5 text-xs select-none transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-primary/45",
                isActive
                  ? "bg-card font-semibold text-primary"
                  : "bg-transparent text-muted-foreground hover:bg-hover-bg hover:text-foreground",
              )}
            >
              {isActive && (
                <span
                  aria-hidden="true"
                  className="absolute inset-x-0 top-0 h-0.5 bg-primary"
                />
              )}
              <span
                className={cn(
                  "shrink-0 font-mono-tech text-[10.5px] font-bold uppercase",
                  methodTextClasses(item.ep.method),
                )}
              >
                {displayMethodBadge(item.ep.method)}
              </span>

              <span className="min-w-0 truncate">
                {item.ep.name || item.ep.path || "endpoint"}
              </span>

              {item.ep.adapter && item.ep.adapter !== "http_json" && (
                <span className="shrink-0 rounded-md border border-border bg-muted px-1 py-0 text-[9.5px] font-mono-tech font-semibold text-muted-foreground">
                  {shortAdapterBadge(item.ep.adapter)}
                </span>
              )}

              {item.runtime.running ? (
                <Loader2 className="h-3 w-3 shrink-0 animate-spin text-primary" />
              ) : (
                <span
                  className={cn(
                    "h-1.5 w-1.5 shrink-0 rounded-full",
                    gateDotClass(item.ep),
                  )}
                />
              )}

              {tabs.length > 1 && (
                <button
                  type="button"
                  title={t("upstreamConnector.closeTab")}
                  aria-label={t("upstreamConnector.closeTab")}
                  onClick={(e) => {
                    e.stopPropagation();
                    closeEndpointTab(item.id);
                  }}
                  className={cn(
                    "inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-active-bg hover:text-foreground transition-opacity cursor-pointer",
                    isActive
                      ? "opacity-75 hover:opacity-100"
                      : "opacity-0 group-hover:opacity-75 hover:opacity-100",
                  )}
                >
                  <X className="h-3 w-3" />
                </button>
              )}
            </div>
          );
        })}
      </div>

      {canWrite && (
        <button
          type="button"
          onClick={() => addEndpoint("")}
          title={t("upstreamConnector.newEndpoint")}
          aria-label={t("upstreamConnector.newEndpoint")}
          data-testid="wb-tab-add"
          className="inline-flex w-9 shrink-0 items-center justify-center border-l border-border text-muted-foreground hover:bg-hover-bg hover:text-foreground transition-colors cursor-pointer"
        >
          <Plus className="h-3.5 w-3.5" />
        </button>
      )}
    </div>
  );
}
