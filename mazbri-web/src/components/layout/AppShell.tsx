import { useEffect } from "react";
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

export function AppShell() {
  const isLogin = useAuthStore((s) => s.isLogin);

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
    <div className="flex h-screen w-full overflow-hidden bg-canvas-bg text-foreground">
      <Sidebar />
      <div className="flex flex-1 flex-col min-w-0 min-h-0 overflow-hidden">
        <Topbar />
        <main className="flex-1 overflow-auto bg-canvas-bg p-4 min-w-0">
          <Outlet />
        </main>
      </div>
      <PersonalSecurityDrawer />
      <LockScreen />
      <WatermarkOverlay />
      <MfaStepUpHost />
    </div>
  );
}
