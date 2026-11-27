import type React from "react";
import { FolderTree, History, ShieldCheck } from "lucide-react";
import { ResizableWorkspace, cn } from "@/components/ui";
import { useT } from "@/locales";
import {
  selectGateView,
  useWorkbenchStore,
  type SidebarPanelId,
} from "../store";
import { EndpointTreePanel } from "../tree";
import { WorkbenchStatusBar } from "./WorkbenchStatusBar";
import { WorkbenchTabStrip } from "./WorkbenchTabStrip";
import { WorkbenchTopBar } from "./WorkbenchTopBar";

export interface WorkbenchShellProps {
  editorPane: React.ReactNode;
  responsePane: React.ReactNode;
  gatePanel: React.ReactNode;
  historyPanel: React.ReactNode;
  onBack: () => void;
  onOpenEnvDrawer: () => void;
}

export function WorkbenchShell({
  editorPane,
  responsePane,
  gatePanel,
  historyPanel,
  onBack,
  onOpenEnvDrawer,
}: WorkbenchShellProps) {
  const { t } = useT();

  const activeSidebarPanel = useWorkbenchStore((s) => s.activeSidebarPanel);
  const sidebarCollapsed = useWorkbenchStore((s) => s.sidebarCollapsed);
  const responseCollapsed = useWorkbenchStore((s) => s.responseCollapsed);
  const responseSplitDir = useWorkbenchStore((s) => s.responseSplitDir);
  const endpointsCount = useWorkbenchStore((s) => s.endpoints.length);
  const runsCount = useWorkbenchStore((s) => s.runs.length);
  const gate = useWorkbenchStore(selectGateView);

  const setSidebarPanel = useWorkbenchStore((s) => s.setSidebarPanel);
  const setSidebarCollapsed = useWorkbenchStore((s) => s.setSidebarCollapsed);

  const effectivePanel: Exclude<SidebarPanelId, "env"> =
    activeSidebarPanel === "gate" || activeSidebarPanel === "history"
      ? activeSidebarPanel
      : "endpoints";

  const railItems: Array<{
    id: Exclude<SidebarPanelId, "env">;
    icon: React.ReactNode;
    label: string;
    badge: string;
  }> = [
    {
      id: "endpoints",
      icon: <FolderTree className="h-4 w-4" />,
      label: t("upstreamConnector.sidebarEndpoints"),
      badge: endpointsCount ? String(endpointsCount) : "",
    },
    {
      id: "gate",
      icon: <ShieldCheck className="h-4 w-4" />,
      label: t("upstreamConnector.sidebarGate"),
      badge: gate.total ? `${gate.ok}/${gate.total}` : "",
    },
    {
      id: "history",
      icon: <History className="h-4 w-4" />,
      label: t("upstreamConnector.sidebarHistory"),
      badge: runsCount ? String(runsCount) : "",
    },
  ];

  const handleRailClick = (id: Exclude<SidebarPanelId, "env">) => {
    if (!sidebarCollapsed && effectivePanel === id) {
      setSidebarCollapsed(true);
      return;
    }
    setSidebarPanel(id);
    if (sidebarCollapsed) setSidebarCollapsed(false);
  };

  const sidebarContent =
    effectivePanel === "gate" ? (
      gatePanel
    ) : effectivePanel === "history" ? (
      historyPanel
    ) : (
      <EndpointTreePanel />
    );

  // Legacy semantics: "horizontal" = top/bottom stacked (ResizableWorkspace vertical),
  // "vertical" = side-by-side (ResizableWorkspace horizontal).
  const innerDirection =
    responseSplitDir === "vertical" ? "horizontal" : "vertical";

  const mainWorkspace = (
    <div className="flex h-full w-full min-h-0 min-w-0 flex-col overflow-hidden bg-canvas-bg">
      <WorkbenchTabStrip />
      <div className="flex-1 min-h-0 min-w-0 overflow-hidden">
        {responseCollapsed ? (
          <div className="flex h-full w-full min-h-0 min-w-0 flex-col overflow-hidden">
            {editorPane}
          </div>
        ) : (
          <ResizableWorkspace
            key={`workbench-inner-${responseSplitDir}`}
            storageKey={`workbench-inner-${responseSplitDir}`}
            direction={innerDirection}
            panes={[
              {
                id: "request",
                defaultSize: 55,
                minSize: 22,
                className: "bg-card",
                children: editorPane,
              },
              {
                id: "response",
                defaultSize: 45,
                minSize: 18,
                className: "bg-card",
                children: responsePane,
              },
            ]}
          />
        )}
      </div>
    </div>
  );

  return (
    <div
      data-testid="wb-shell"
      className="flex h-screen w-full flex-col overflow-hidden bg-canvas-bg text-foreground"
    >
      <WorkbenchTopBar onBack={onBack} onOpenEnvDrawer={onOpenEnvDrawer} />

      <main className="flex flex-1 min-h-0 min-w-0 overflow-hidden">
        {/* Left icon rail (Endpoints / Gate / History; Env lives exclusively in TopBar per A-12) */}
        <nav
          aria-label={t("upstreamConnector.workbenchTitle")}
          className="flex w-11 shrink-0 flex-col items-center gap-1 border-r border-border bg-card py-2"
        >
          {railItems.map((item) => {
            const active = !sidebarCollapsed && effectivePanel === item.id;
            return (
              <button
                key={item.id}
                type="button"
                title={item.label}
                aria-label={item.label}
                aria-pressed={active}
                data-testid={`wb-rail-${item.id}`}
                onClick={() => handleRailClick(item.id)}
                className={cn(
                  "relative inline-flex h-9 w-9 flex-col items-center justify-center gap-0.5 rounded-md transition-colors cursor-pointer",
                  active
                    ? "bg-primary-soft text-primary"
                    : "text-muted-foreground hover:bg-hover-bg hover:text-foreground",
                )}
              >
                {active && (
                  <span
                    aria-hidden="true"
                    className="absolute inset-y-1.5 -left-1 w-0.5 rounded-full bg-primary"
                  />
                )}
                {item.icon}
                {item.badge && (
                  <span
                    className={cn(
                      "font-mono-tech text-[9px] font-semibold leading-none tabular-nums",
                      active ? "text-primary" : "text-muted-foreground",
                    )}
                  >
                    {item.badge}
                  </span>
                )}
              </button>
            );
          })}
        </nav>

        <div className="flex flex-1 min-h-0 min-w-0 overflow-hidden">
          {sidebarCollapsed ? (
            mainWorkspace
          ) : (
            <ResizableWorkspace
              storageKey="workbench-outer"
              direction="horizontal"
              panes={[
                {
                  id: "sidebar",
                  defaultSize: 22,
                  defaultPx: 260,
                  minSize: 14,
                  minPx: 200,
                  maxSize: 42,
                  className: "bg-card border-r border-border",
                  children: sidebarContent,
                },
                {
                  id: "main",
                  defaultSize: 78,
                  minSize: 45,
                  children: mainWorkspace,
                },
              ]}
            />
          )}
        </div>
      </main>

      <WorkbenchStatusBar />
    </div>
  );
}
