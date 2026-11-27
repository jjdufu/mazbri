import { useMemo, useState } from "react";
import {
  ChevronDown,
  ChevronRight,
  Copy,
  Layers,
  MoreVertical,
  Plus,
  Search,
  ShieldCheck,
  Trash2,
  X,
} from "lucide-react";
import { IconButton, Input, Popover, cn } from "@/components/ui";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import { canWriteUpstreamConnector } from "@/utils/ops/roles";
import type { ConnectorEndpoint } from "@/api/upstream-connectors";
import {
  displayMethodBadge,
  groupEndpointsByTag,
} from "../engine/workbenchHelpers";
import {
  selectActiveEpIdx,
  selectGateView,
  useWorkbenchStore,
} from "../store";
import { methodBadgeClasses } from "../layout/methodStyles";

export function EndpointTreePanel() {
  const { t } = useT();
  const role = useAuthStore((s) => s.info.role);
  const canWrite = canWriteUpstreamConnector(role);

  const endpoints = useWorkbenchStore((s) => s.endpoints);
  const activeIndex = useWorkbenchStore(selectActiveEpIdx);
  const gate = useWorkbenchStore(selectGateView);
  const addEndpoint = useWorkbenchStore((s) => s.addEndpoint);
  const switchEndpointByIndex = useWorkbenchStore(
    (s) => s.switchEndpointByIndex,
  );
  const toggleEndpointGate = useWorkbenchStore((s) => s.toggleEndpointGate);
  const duplicateEndpoint = useWorkbenchStore((s) => s.duplicateEndpoint);
  const deleteEndpoint = useWorkbenchStore((s) => s.deleteEndpoint);

  const [search, setSearch] = useState("");
  const [closedFolders, setClosedFolders] = useState<Record<string, boolean>>(
    {},
  );
  const [openMenuId, setOpenMenuId] = useState<string | null>(null);

  const groupedEndpoints = useMemo(
    () => groupEndpointsByTag(endpoints, search),
    [endpoints, search],
  );

  const isFolderOpen = (tag: string) => !closedFolders[tag || "__default__"];

  const toggleFolder = (tag: string) => {
    const key = tag || "__default__";
    setClosedFolders((prev) => ({ ...prev, [key]: !prev[key] }));
  };

  const gateDotClass = (ep: ConnectorEndpoint) => {
    if (ep.gate === false) {
      return "bg-transparent border border-muted-foreground/60";
    }
    const hit = gate.items.find((g) => g.id === ep.id || g.name === ep.name);
    return hit?.ok ? "bg-success-fg" : "bg-warning-fg";
  };

  return (
    <div className="flex h-full w-full flex-col min-h-0 min-w-0 bg-card text-foreground">
      <div className="flex h-9 shrink-0 items-center justify-between border-b border-border px-3">
        <span className="text-xs font-semibold text-foreground truncate">
          {t("upstreamConnector.sidebarEndpoints")}
        </span>
        {canWrite && (
          <IconButton
            size="sm"
            onClick={() => addEndpoint("")}
            title={t("upstreamConnector.newEndpoint")}
            aria-label={t("upstreamConnector.newEndpoint")}
            data-testid="wb-tree-add-endpoint"
          >
            <Plus className="h-3.5 w-3.5" />
          </IconButton>
        )}
      </div>

      <div className="shrink-0 border-b border-border bg-muted/40 p-2">
        <div className="relative flex items-center">
          <Search className="pointer-events-none absolute left-2.5 h-3.5 w-3.5 text-muted-foreground" />
          <Input
            type="text"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t("upstreamConnector.searchEndpoints")}
            aria-label={t("upstreamConnector.searchEndpoints")}
            className="h-8 rounded-md pl-8 pr-7 text-xs"
          />
          {search && (
            <button
              type="button"
              onClick={() => setSearch("")}
              aria-label={t("upstreamConnector.clearKv")}
              className="absolute right-1.5 inline-flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground hover:bg-hover-bg hover:text-foreground cursor-pointer"
            >
              <X className="h-3 w-3" />
            </button>
          )}
        </div>
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto">
        {groupedEndpoints.length === 0 ? (
          <div className="px-3 py-8 text-center text-xs text-muted-foreground">
            {t("upstreamConnector.noEndpoints")}
          </div>
        ) : (
          groupedEndpoints.map((group) => {
            const folderKey = group.tag || "__default__";
            const open = isFolderOpen(group.tag);
            return (
              <div
                key={folderKey}
                className="border-b border-border-subtle last:border-b-0"
              >
                <div className="flex h-7 items-center justify-between gap-1 bg-muted/45 px-2">
                  <button
                    type="button"
                    onClick={() => toggleFolder(group.tag)}
                    className="flex flex-1 min-w-0 items-center gap-1.5 text-left text-xs font-medium text-muted-foreground hover:text-foreground cursor-pointer"
                  >
                    {open ? (
                      <ChevronDown className="h-3.5 w-3.5 shrink-0" />
                    ) : (
                      <ChevronRight className="h-3.5 w-3.5 shrink-0" />
                    )}
                    <Layers className="h-3 w-3 shrink-0" />
                    <span className="truncate">
                      {group.tag || t("upstreamConnector.untaggedGroup")}
                    </span>
                    <span className="ml-auto rounded-full bg-card border border-border px-1.5 py-0 text-[10px] font-mono-tech tabular-nums text-muted-foreground">
                      {group.items.length}
                    </span>
                  </button>
                  {canWrite && (
                    <button
                      type="button"
                      onClick={() => addEndpoint(group.tag)}
                      title={t("upstreamConnector.newEndpoint")}
                      aria-label={t("upstreamConnector.newEndpoint")}
                      className="inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-hover-bg hover:text-foreground cursor-pointer"
                    >
                      <Plus className="h-3 w-3" />
                    </button>
                  )}
                </div>

                {open && (
                  <div className="py-1 space-y-0.5 px-1">
                    {group.items.map(({ ep, index }) => {
                      const epKey = ep.id || `ep-${index}`;
                      const isActive = index === activeIndex;
                      const menuOpen = openMenuId === epKey;
                      return (
                        <div
                          key={epKey}
                          role="button"
                          tabIndex={0}
                          onClick={() => switchEndpointByIndex(index)}
                          onKeyDown={(e) => {
                            if (e.key === "Enter" || e.key === " ") {
                              e.preventDefault();
                              switchEndpointByIndex(index);
                            }
                          }}
                          data-testid={`wb-ep-item-${index}`}
                          className={cn(
                            "group flex items-center gap-2 rounded-md px-2 py-1.5 text-xs transition-colors cursor-pointer select-none",
                            isActive
                              ? "bg-primary-soft text-primary"
                              : "text-foreground hover:bg-hover-bg",
                          )}
                        >
                          <span
                            className={cn(
                              "inline-flex shrink-0 items-center justify-center rounded-md border px-1.5 py-0.5 text-[10px] font-mono-tech font-bold leading-none uppercase min-w-[38px]",
                              methodBadgeClasses(ep.method),
                            )}
                          >
                            {displayMethodBadge(ep.method)}
                          </span>

                          <div className="min-w-0 flex-1">
                            <div
                              className={cn(
                                "truncate text-xs leading-tight",
                                isActive
                                  ? "font-semibold text-primary"
                                  : "font-medium text-foreground",
                              )}
                            >
                              {ep.name || ep.path || "endpoint"}
                            </div>
                            <div className="truncate font-mono-tech text-[11px] text-muted-foreground leading-tight mt-0.5">
                              {ep.path || "/"}
                            </div>
                          </div>

                          <button
                            type="button"
                            disabled={!canWrite}
                            title={
                              ep.gate === false
                                ? t("upstreamConnector.gateOffHint")
                                : t("upstreamConnector.gateOnHint")
                            }
                            aria-label={
                              ep.gate === false
                                ? t("upstreamConnector.gateOffHint")
                                : t("upstreamConnector.gateOnHint")
                            }
                            onClick={(e) => {
                              e.stopPropagation();
                              if (canWrite) toggleEndpointGate(index);
                            }}
                            className="inline-flex h-6 w-4 shrink-0 items-center justify-center rounded-md disabled:cursor-default cursor-pointer"
                          >
                            <span
                              className={cn(
                                "h-2 w-2 rounded-full",
                                gateDotClass(ep),
                              )}
                            />
                          </button>

                          {canWrite && (
                            <Popover
                              open={menuOpen}
                              onOpenChange={(next) =>
                                setOpenMenuId(next ? epKey : null)
                              }
                              align="right"
                              trigger={
                                <button
                                  type="button"
                                  aria-label={t(
                                    "upstreamConnector.sessionActions",
                                  )}
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    setOpenMenuId(menuOpen ? null : epKey);
                                  }}
                                  className={cn(
                                    "inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-active-bg hover:text-foreground transition-opacity cursor-pointer",
                                    menuOpen
                                      ? "opacity-100"
                                      : "opacity-0 group-hover:opacity-100 focus:opacity-100",
                                  )}
                                >
                                  <MoreVertical className="h-3.5 w-3.5" />
                                </button>
                              }
                              contentClassName="w-36 p-1 space-y-0.5"
                            >
                              <button
                                type="button"
                                onClick={(e) => {
                                  e.stopPropagation();
                                  setOpenMenuId(null);
                                  duplicateEndpoint(index);
                                }}
                                className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-xs text-foreground hover:bg-hover-bg cursor-pointer"
                              >
                                <Copy className="h-3.5 w-3.5 text-muted-foreground" />
                                <span>{t("upstreamConnector.duplicate")}</span>
                              </button>
                              <button
                                type="button"
                                onClick={(e) => {
                                  e.stopPropagation();
                                  setOpenMenuId(null);
                                  toggleEndpointGate(index);
                                }}
                                className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-xs text-foreground hover:bg-hover-bg cursor-pointer"
                              >
                                <ShieldCheck className="h-3.5 w-3.5 text-muted-foreground" />
                                <span>{t("upstreamConnector.gateShort")}</span>
                              </button>
                              {endpoints.length > 1 && (
                                <>
                                  <div className="my-1 h-px bg-border" />
                                  <button
                                    type="button"
                                    onClick={(e) => {
                                      e.stopPropagation();
                                      setOpenMenuId(null);
                                      deleteEndpoint(index);
                                    }}
                                    className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-xs text-danger-fg hover:bg-danger-fg/10 cursor-pointer"
                                  >
                                    <Trash2 className="h-3.5 w-3.5" />
                                    <span>{t("common.actions.delete")}</span>
                                  </button>
                                </>
                              )}
                            </Popover>
                          )}
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
