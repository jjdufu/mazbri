import { useEffect, useState } from "react";
import { Outlet } from "react-router";
import { useAuthStore } from "@/stores/auth";
import { useMfaStepUpStore } from "@/stores/mfa-step-up";
import { useWatermarkStore } from "@/stores/watermark";
import { LockScreen } from "./LockScreen";
import { MfaStepUpHost } from "./MfaStepUpHost";
import { PersonalSecurityDrawer } from "./PersonalSecurityDrawer";
import { Sidebar } from "./Sidebar";
import { Topbar } from "./Topbar";
import { WatermarkOverlay } from "./WatermarkOverlay";

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

  return (
    <div className="flex h-screen w-full overflow-hidden bg-app-frame text-foreground">
      <Sidebar
        collapsed={collapsed}
        onToggleCollapse={() => setCollapsed((prev) => !prev)}
      />

      {/* Right-Hand Floating Rounded Inset Workspace Sheet (Scheme A) */}
      <div className="flex-1 min-w-0 flex flex-col bg-canvas-bg md:my-2 md:mr-2 md:rounded-lg md:shadow-[0_1px_3px_rgba(15,23,42,0.05),0_0_0_1px_var(--border)] dark:md:shadow-[0_0_0_1px_var(--border)] overflow-hidden">
        <Topbar />
        <main className="flex flex-col flex-1 overflow-auto px-5 pb-5 pt-2 md:px-6 md:pb-6 min-w-0">
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
