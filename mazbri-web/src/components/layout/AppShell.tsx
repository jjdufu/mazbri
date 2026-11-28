import { useEffect, useMemo, useState } from "react";
import { Outlet, useLocation } from "react-router";
import { PageHeader } from "@/components/ui";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import { useLockScreenStore } from "@/stores/lockscreen";
import { useMfaStepUpStore } from "@/stores/mfa-step-up";
import { useWatermarkStore } from "@/stores/watermark";
import { GlobalCommandPalette } from "./GlobalCommandPalette";
import { LockScreen } from "./LockScreen";
import { MfaStepUpHost } from "./MfaStepUpHost";
import { PageHeaderSlotContext } from "./PageHeaderActions";
import { PersonalSecurityDrawer } from "./PersonalSecurityDrawer";
import { Sidebar } from "./Sidebar";
import { Topbar } from "./Topbar";
import { WatermarkOverlay } from "./WatermarkOverlay";
import { resolveMenuPageHeader } from "./menuConfig";

const SIDEBAR_COLLAPSED_KEY = "mazbri-sidebar-collapsed";

function readInitialCollapsed(): boolean {
  if (typeof localStorage === "undefined") return false;
  try {
    return localStorage.getItem(SIDEBAR_COLLAPSED_KEY) === "1";
  } catch {
    return false;
  }
}

export function AppShell() {
  const location = useLocation();
  const { t } = useT();
  const isLogin = useAuthStore((s) => s.isLogin);
  const isLocked = useLockScreenStore((s) => s.isLocked);
  const [collapsed, setCollapsed] = useState<boolean>(readInitialCollapsed);
  const [cmdkOpen, setCmdkOpen] = useState(false);
  const [headerActionsEl, setHeaderActionsEl] = useState<HTMLDivElement | null>(
    null,
  );

  const pageHeader = useMemo(
    () => resolveMenuPageHeader(location.pathname),
    [location.pathname],
  );

  const headerSlotValue = useMemo(
    () => ({
      hasHost: Boolean(pageHeader),
      slotEl: pageHeader ? headerActionsEl : null,
    }),
    [pageHeader, headerActionsEl],
  );

  useEffect(() => {
    if (typeof localStorage === "undefined") return;
    try {
      localStorage.setItem(SIDEBAR_COLLAPSED_KEY, collapsed ? "1" : "0");
    } catch {
      // Ignore storage errors in restricted contexts.
    }
  }, [collapsed]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (isLocked) return;
      if ((e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey) {
        const key = e.key.toLowerCase();
        if (key === "k") {
          e.preventDefault();
          setCmdkOpen((prev) => !prev);
          return;
        }
        if (key === "b") {
          const tag = (e.target as HTMLElement | null)?.tagName;
          if (
            tag === "INPUT" ||
            tag === "TEXTAREA" ||
            (e.target as HTMLElement | null)?.isContentEditable
          ) {
            return;
          }
          e.preventDefault();
          setCollapsed((prev) => !prev);
        }
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isLocked]);

  useEffect(() => {
    if (!isLogin) return;
    void useWatermarkStore
      .getState()
      .loadFromServer()
      .catch(() => undefined);
    void useMfaStepUpStore
      .getState()
      .refresh()
      .catch(() => undefined);
  }, [isLogin]);

  return (
    <div className="flex h-screen w-full overflow-hidden bg-app-frame text-foreground">
      <Sidebar
        collapsed={collapsed}
        onToggleCollapse={() => setCollapsed((prev) => !prev)}
      />

      {/* Right-Hand Floating Rounded Inset Workspace Sheet (Scheme A) */}
      <div className="flex-1 min-w-0 flex flex-col bg-canvas-bg md:my-2 md:mr-2 md:rounded-lg md:shadow-[0_1px_3px_rgba(15,23,42,0.05),0_0_0_1px_var(--border)] dark:md:shadow-[0_0_0_1px_var(--border)] overflow-hidden">
        <Topbar onOpenCommandPalette={() => setCmdkOpen(true)} />
        <main className="flex flex-col flex-1 overflow-auto px-5 pb-5 pt-2 md:px-6 md:pb-6 md:pt-2.5 min-w-0">
          {pageHeader && (
            <PageHeader
              title={t(pageHeader.titleKey)}
              actions={
                <div
                  ref={setHeaderActionsEl}
                  className="flex flex-wrap items-center justify-end gap-2 empty:hidden"
                />
              }
              className="mb-2.5 shrink-0"
            />
          )}
          <PageHeaderSlotContext.Provider value={headerSlotValue}>
            <div className="flex flex-col flex-1 min-h-0 min-w-0">
              <Outlet />
            </div>
          </PageHeaderSlotContext.Provider>
        </main>
      </div>

      <GlobalCommandPalette open={cmdkOpen} onOpenChange={setCmdkOpen} />
      <PersonalSecurityDrawer />
      <LockScreen />
      <WatermarkOverlay />
      <MfaStepUpHost />
    </div>
  );
}
