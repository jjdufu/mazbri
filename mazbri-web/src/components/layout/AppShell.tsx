import { useEffect, useMemo, useState } from "react";
import { Outlet, useLocation } from "react-router";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import { useMfaStepUpStore } from "@/stores/mfa-step-up";
import { useWatermarkStore } from "@/stores/watermark";
import { LockScreen } from "./LockScreen";
import { MfaStepUpHost } from "./MfaStepUpHost";
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
  const [collapsed, setCollapsed] = useState<boolean>(readInitialCollapsed);

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
      if ((e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "b") {
        const tag = (e.target as HTMLElement | null)?.tagName;
        if (tag === "INPUT" || tag === "TEXTAREA" || (e.target as HTMLElement | null)?.isContentEditable) {
          return;
        }
        e.preventDefault();
        setCollapsed((prev) => !prev);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

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

  const pageHeader = useMemo(
    () => resolveMenuPageHeader(location.pathname),
    [location.pathname],
  );

  return (
    <div className="flex flex-col h-screen w-full overflow-hidden bg-background text-foreground">
      <Topbar
        collapsed={collapsed}
        onToggleCollapse={() => setCollapsed((prev) => !prev)}
      />
      <div className="flex flex-1 min-h-0 overflow-hidden">
        <Sidebar collapsed={collapsed} />
        <main className="flex flex-col flex-1 overflow-auto bg-canvas-bg px-6 py-5 min-w-0">
          {pageHeader && (
            <div className="mb-4 flex items-center gap-2 shrink-0">
              <h1 className="text-base font-semibold tracking-tight text-foreground">
                {t(pageHeader.titleKey)}
              </h1>
            </div>
          )}
          <div className="flex flex-col flex-1 min-h-0 min-w-0">
            <Outlet />
          </div>
        </main>
      </div>
      <PersonalSecurityDrawer />
      <LockScreen />
      <WatermarkOverlay />
      <MfaStepUpHost />
    </div>
  );
}
