import {
  Columns2,
  Command,
  Loader2,
  PanelBottom,
  PanelLeft,
  Rows2,
  ScanLine,
} from "lucide-react";
import { cn } from "@/components/ui";
import { useT } from "@/locales";
import { selectActiveRuntime, useWorkbenchStore } from "../store";

export function WorkbenchStatusBar() {
  const { t } = useT();

  const sidebarCollapsed = useWorkbenchStore((s) => s.sidebarCollapsed);
  const responseCollapsed = useWorkbenchStore((s) => s.responseCollapsed);
  const responseSplitDir = useWorkbenchStore((s) => s.responseSplitDir);
  const capturedVars = useWorkbenchStore((s) => s.capturedVars);
  const activeRuntime = useWorkbenchStore(selectActiveRuntime);

  const toggleSidebar = useWorkbenchStore((s) => s.toggleSidebar);
  const toggleResponseCollapsed = useWorkbenchStore(
    (s) => s.toggleResponseCollapsed,
  );
  const toggleSplitDirection = useWorkbenchStore((s) => s.toggleSplitDirection);
  const setReqTab = useWorkbenchStore((s) => s.setReqTab);
  const setShowShortcuts = useWorkbenchStore((s) => s.setShowShortcuts);

  const capturedCount = Object.keys(capturedVars).length;
  const { running, lastRun } = activeRuntime;

  return (
    <footer
      data-testid="wb-statusbar"
      className="flex h-7 shrink-0 items-center justify-between gap-2 border-t border-border bg-card px-2 text-[11.5px] text-muted-foreground"
    >
      {/* Left: Layout toggles + optional captures chip (A-12: no duplicate Global Env trigger here) */}
      <div className="flex items-center gap-1">
        <button
          type="button"
          onClick={toggleSidebar}
          title={
            sidebarCollapsed
              ? t("upstreamConnector.expandSidebar")
              : t("upstreamConnector.collapseSidebar")
          }
          aria-label={
            sidebarCollapsed
              ? t("upstreamConnector.expandSidebar")
              : t("upstreamConnector.collapseSidebar")
          }
          data-testid="wb-status-toggle-sidebar"
          className={cn(
            "inline-flex h-6 items-center gap-1 rounded-md px-1.5 transition-colors cursor-pointer",
            !sidebarCollapsed
              ? "text-foreground hover:bg-hover-bg"
              : "text-muted-foreground hover:bg-hover-bg hover:text-foreground",
          )}
        >
          <PanelLeft className="h-3.5 w-3.5" />
        </button>

        <button
          type="button"
          onClick={toggleResponseCollapsed}
          title={
            responseCollapsed
              ? t("upstreamConnector.expandResponse")
              : t("upstreamConnector.collapseResponse")
          }
          aria-label={
            responseCollapsed
              ? t("upstreamConnector.expandResponse")
              : t("upstreamConnector.collapseResponse")
          }
          data-testid="wb-status-toggle-response"
          className={cn(
            "inline-flex h-6 items-center gap-1 rounded-md px-1.5 transition-colors cursor-pointer",
            !responseCollapsed
              ? "text-foreground hover:bg-hover-bg"
              : "text-muted-foreground hover:bg-hover-bg hover:text-foreground",
          )}
        >
          <PanelBottom className="h-3.5 w-3.5" />
        </button>

        <button
          type="button"
          onClick={toggleSplitDirection}
          title={
            responseSplitDir === "vertical"
              ? t("upstreamConnector.splitTopBottom")
              : t("upstreamConnector.splitSideBySide")
          }
          aria-label={
            responseSplitDir === "vertical"
              ? t("upstreamConnector.splitTopBottom")
              : t("upstreamConnector.splitSideBySide")
          }
          data-testid="wb-status-toggle-split"
          className="inline-flex h-6 items-center gap-1 rounded-md px-1.5 text-muted-foreground hover:bg-hover-bg hover:text-foreground transition-colors cursor-pointer"
        >
          {responseSplitDir === "vertical" ? (
            <Columns2 className="h-3.5 w-3.5" />
          ) : (
            <Rows2 className="h-3.5 w-3.5" />
          )}
        </button>

        {capturedCount > 0 && (
          <>
            <span className="mx-0.5 h-3 w-px bg-border" aria-hidden="true" />
            <button
              type="button"
              onClick={() => setReqTab("captures")}
              data-testid="wb-status-captures"
              className="inline-flex h-6 items-center gap-1 rounded-md px-1.5 text-success-fg hover:bg-hover-bg tabular-nums transition-colors cursor-pointer"
            >
              <ScanLine className="h-3 w-3" />
              <span>captures · {capturedCount}</span>
            </button>
          </>
        )}
      </div>

      {/* Right: Run status + Shortcuts */}
      <div className="flex items-center gap-1">
        <button
          type="button"
          onClick={toggleResponseCollapsed}
          data-testid="wb-status-run"
          className="inline-flex h-6 items-center gap-1.5 rounded-md px-2 text-muted-foreground hover:bg-hover-bg hover:text-foreground tabular-nums transition-colors cursor-pointer"
        >
          {running ? (
            <>
              <Loader2 className="h-3 w-3 animate-spin text-primary" />
              <span>{t("upstreamConnector.requesting")}</span>
            </>
          ) : lastRun ? (
            <>
              <span
                className={cn(
                  "h-1.5 w-1.5 rounded-full",
                  lastRun.ok ? "bg-success-fg" : "bg-danger-fg",
                )}
              />
              <span>
                {lastRun.response_preview?.status ??
                  (lastRun.ok ? "OK" : "ERR")}{" "}
                · {lastRun.latency_ms ?? 0} ms
              </span>
            </>
          ) : (
            <span>{t("upstreamConnector.statusReady")}</span>
          )}
        </button>

        <span className="mx-0.5 h-3 w-px bg-border" aria-hidden="true" />

        <button
          type="button"
          onClick={() => setShowShortcuts(true)}
          title={t("upstreamConnector.shortcutsTitle")}
          data-testid="wb-status-shortcuts"
          className="inline-flex h-6 items-center gap-1 rounded-md px-1.5 text-muted-foreground hover:bg-hover-bg hover:text-foreground transition-colors cursor-pointer"
        >
          <Command className="h-3 w-3" />
          <span>{t("upstreamConnector.shortcutsTitle")}</span>
        </button>
      </div>
    </footer>
  );
}
