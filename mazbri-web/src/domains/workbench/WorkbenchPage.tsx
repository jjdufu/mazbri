import { useCallback, useEffect, useRef } from "react";
import { useNavigate, useSearchParams } from "react-router";
import { Loader2, Plus } from "lucide-react";
import {
  LockScreen,
  MfaStepUpHost,
  PersonalSecurityDrawer,
  WatermarkOverlay,
} from "@/components/layout";
import { Button, EmptyState } from "@/components/ui";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import { useMfaStepUpStore } from "@/stores/mfa-step-up";
import { useWatermarkStore } from "@/stores/watermark";
import {
  WorkbenchCommandPalette,
  WorkbenchConfirmHost,
  WorkbenchEnvDrawer,
  WorkbenchImportDialog,
  WorkbenchPublishDrawer,
  WorkbenchShortcutsModal,
} from "./dialogs";
import {
  WorkbenchEditorPane,
  type WorkbenchEnvTextHandle,
} from "./editors";
import { WorkbenchShell, WorkbenchTopBar } from "./layout";
import {
  WorkbenchGatePanel,
  WorkbenchHistoryPanel,
  WorkbenchResponsePane,
  useWorkbenchHotkeys,
} from "./runtime";
import {
  selectCanWriteWorkbench,
  selectDirty,
  useWorkbenchStore,
} from "./store";

export function WorkbenchPage() {
  const { t } = useT();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const urlInputRef = useRef<WorkbenchEnvTextHandle | null>(null);
  const bootstrappedRef = useRef(false);
  const prevQueryRef = useRef<{ supplierId: string; sessionId: string } | null>(
    null,
  );

  const isLogin = useAuthStore((s) => s.isLogin);
  const draft = useWorkbenchStore((s) => s.draft);
  const bootstrapping = useWorkbenchStore((s) => s.bootstrapping);
  const canWrite = useWorkbenchStore(selectCanWriteWorkbench);
  const setEnvDrawerOpen = useWorkbenchStore((s) => s.setEnvDrawerOpen);
  const bootstrap = useWorkbenchStore((s) => s.bootstrap);
  const switchToSupplier = useWorkbenchStore((s) => s.switchToSupplier);
  const switchToDraft = useWorkbenchStore((s) => s.switchToDraft);
  const createBlankSession = useWorkbenchStore((s) => s.createBlankSession);
  const confirmDiscardIfDirty = useWorkbenchStore(
    (s) => s.confirmDiscardIfDirty,
  );
  const abortAllRuns = useWorkbenchStore((s) => s.abortAllRuns);

  useWorkbenchHotkeys();

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

  const supplierId = (
    searchParams.get("supplier_id") ||
    searchParams.get("supplierId") ||
    ""
  ).trim();
  const sessionId = (
    searchParams.get("session") ||
    searchParams.get("draftId") ||
    searchParams.get("draft_id") ||
    ""
  ).trim();

  useEffect(() => {
    if (!bootstrappedRef.current) {
      bootstrappedRef.current = true;
      prevQueryRef.current = { supplierId, sessionId };
      void bootstrap(
        {
          supplierId: supplierId || undefined,
          sessionId: sessionId || undefined,
        },
        navigate,
      );
      return;
    }

    const prev = prevQueryRef.current;
    prevQueryRef.current = { supplierId, sessionId };
    if (
      prev &&
      prev.supplierId === supplierId &&
      prev.sessionId === sessionId
    ) {
      return;
    }

    const cur = useWorkbenchStore.getState();
    if (cur.bootstrapping) return;

    if (supplierId) {
      if (cur.draft?.linked_supplier_id === supplierId) return;
      void switchToSupplier(supplierId, navigate);
      return;
    }
    if (sessionId) {
      if (cur.draft?.id === sessionId) return;
      void switchToDraft(sessionId, navigate);
      return;
    }
    if (!cur.draft || cur.draft.linked_supplier_id || cur.draft.id) {
      void createBlankSession(navigate);
    }
  }, [
    supplierId,
    sessionId,
    bootstrap,
    switchToSupplier,
    switchToDraft,
    createBlankSession,
    navigate,
  ]);

  useEffect(() => {
    const onBeforeUnload = (e: BeforeUnloadEvent) => {
      if (!selectDirty(useWorkbenchStore.getState())) return;
      e.preventDefault();
      e.returnValue = "";
    };
    window.addEventListener("beforeunload", onBeforeUnload);
    return () => {
      window.removeEventListener("beforeunload", onBeforeUnload);
      abortAllRuns();
    };
  }, [abortAllRuns]);

  const handleBack = useCallback(() => {
    void (async () => {
      const ok = await confirmDiscardIfDirty();
      if (!ok) return;
      const linkedId = useWorkbenchStore.getState().draft?.linked_supplier_id;
      if (linkedId) {
        navigate(`/operations/upstream-supply/suppliers/${linkedId}`);
        return;
      }
      navigate("/operations/upstream-supply?tab=suppliers");
    })();
  }, [confirmDiscardIfDirty, navigate]);

  const handleOpenEnvDrawer = useCallback(() => {
    setEnvDrawerOpen(true);
  }, [setEnvDrawerOpen]);

  return (
    <>
      {bootstrapping && !draft ? (
        <div
          data-testid="wb-loading"
          className="flex h-screen w-full flex-col overflow-hidden bg-canvas-bg text-foreground"
        >
          <WorkbenchTopBar
            onBack={handleBack}
            onOpenEnvDrawer={handleOpenEnvDrawer}
          />
          <main className="flex flex-1 items-center justify-center p-6">
            <div className="inline-flex items-center gap-2 text-xs text-muted-foreground">
              <Loader2 className="h-4 w-4 animate-spin text-primary" />
              <span>{t("layout.loading")}</span>
            </div>
          </main>
        </div>
      ) : !draft ? (
        <div
          data-testid="wb-empty-shell"
          className="flex h-screen w-full flex-col overflow-hidden bg-canvas-bg text-foreground"
        >
          <WorkbenchTopBar
            onBack={handleBack}
            onOpenEnvDrawer={handleOpenEnvDrawer}
          />
          <main className="flex flex-1 items-center justify-center p-6">
            <EmptyState
              title={t("upstreamConnector.emptyHint")}
              className="w-full max-w-md bg-card"
              action={
                canWrite ? (
                  <Button
                    type="button"
                    variant="primary"
                    size="md"
                    onClick={() => {
                      void createBlankSession(navigate);
                    }}
                  >
                    <Plus className="h-3.5 w-3.5" />
                    {t("upstreamConnector.newSession")}
                  </Button>
                ) : undefined
              }
            />
          </main>
        </div>
      ) : (
        <WorkbenchShell
          editorPane={<WorkbenchEditorPane urlInputRef={urlInputRef} />}
          responsePane={<WorkbenchResponsePane />}
          gatePanel={<WorkbenchGatePanel />}
          historyPanel={<WorkbenchHistoryPanel />}
          onBack={handleBack}
          onOpenEnvDrawer={handleOpenEnvDrawer}
        />
      )}

      <WorkbenchEnvDrawer />
      <WorkbenchImportDialog />
      <WorkbenchPublishDrawer />
      <WorkbenchShortcutsModal />
      <WorkbenchCommandPalette />
      <WorkbenchConfirmHost />

      <PersonalSecurityDrawer />
      <LockScreen />
      <WatermarkOverlay />
      <MfaStepUpHost />
    </>
  );
}
