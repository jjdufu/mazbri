import { type FormEvent, useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { Button, ConfirmDialog, FormField, Input, Modal } from "@/components/ui";
import { useT } from "@/locales";
import { usePersonalSecurityStore } from "@/stores/personal-security";
import {
  bindMfaSetupPrompt,
  bindMfaStepUpDialog,
  cancelMfaDialog,
  readMfaDialogView,
  submitMfaDialog,
  type MfaDialogViewModel,
  type MfaSetupPromptRequest,
} from "@/utils/http/mfa-dialog-service";
import type { MfaCategory } from "@/utils/http/mfa-step-up-state";

interface SetupPromptState {
  request: MfaSetupPromptRequest;
  resolve: () => void;
}

export function MfaStepUpHost() {
  const { t } = useT();
  const navigate = useNavigate();
  const [view, setView] = useState<MfaDialogViewModel>(() =>
    readMfaDialogView(),
  );
  const [code, setCode] = useState("");
  const seenSessionRef = useRef<number>(0);
  const [setupPrompt, setSetupPrompt] = useState<SetupPromptState | null>(null);

  useEffect(() => {
    const unbindDialog = bindMfaStepUpDialog(() => {
      setView(readMfaDialogView());
    });
    const unbindSetup = bindMfaSetupPrompt(
      (request) =>
        new Promise<void>((resolve) => {
          setSetupPrompt({ request, resolve });
        }),
    );
    return () => {
      unbindDialog();
      unbindSetup();
    };
  }, []);

  useEffect(() => {
    if (view.sessionId !== seenSessionRef.current) {
      seenSessionRef.current = view.sessionId;
      setCode("");
    }
  }, [view.sessionId]);

  const resolveCategoryLabel = (category: MfaCategory | ""): string => {
    switch (category) {
      case "certificate":
        return t("common.mfaDialog.categories.certificate");
      case "admin":
        return t("common.mfaDialog.categories.admin");
      case "mfa":
        return t("common.mfaDialog.categories.mfa");
      case "key":
        return t("common.mfaDialog.categories.key");
      case "finance":
        return t("common.mfaDialog.categories.finance");
      case "platform":
        return t("common.mfaDialog.categories.platform");
      default:
        return "";
    }
  };

  const categoryLabel = resolveCategoryLabel(view.category);

  const errorText = (() => {
    const errorCode = view.errorCode;
    if (!errorCode) return undefined;
    if (errorCode === "invalid_format") {
      return t("common.mfaDialog.invalidFormat", { digits: view.totpDigits });
    }
    if (errorCode === "MFA_CODE_REUSED") {
      return t("common.mfaDialog.reused");
    }
    if (errorCode === "MFA_INVALID_CODE" || errorCode === "MFA_INVALID_REQUEST") {
      if (view.attemptsRemaining != null) {
        return t("common.mfaDialog.invalidWithAttempts", {
          n: view.attemptsRemaining,
        });
      }
      return t("common.mfaDialog.invalidCode");
    }
    return t("common.mfaDialog.invalidCode");
  })();

  const handleSubmit = (event?: FormEvent) => {
    event?.preventDefault();
    submitMfaDialog(code);
  };

  const handleCancel = () => {
    if (view.submitting) return;
    cancelMfaDialog(view.sessionId);
  };

  const handleSetupPromptClose = () => {
    if (!setupPrompt) return;
    const { resolve } = setupPrompt;
    setSetupPrompt(null);
    resolve();
  };

  const handleSetupPromptConfirm = () => {
    if (!setupPrompt) return;
    const { request, resolve } = setupPrompt;
    setSetupPrompt(null);
    if (request.openPersonalSecurityDrawer) {
      usePersonalSecurityStore.getState().openDrawer("mfa");
    } else if (request.resolvedPath) {
      navigate(request.resolvedPath);
    }
    resolve();
  };

  return (
    <>
      <Modal
        open={view.visible}
        onClose={handleCancel}
        size="sm"
        closeOnBackdrop={false}
        testId="mfa-step-up-modal"
        title={t("common.mfaDialog.title")}
        footer={
          <>
            <Button
              type="button"
              variant="outline"
              disabled={view.submitting}
              onClick={handleCancel}
              data-testid="mfa-step-up-cancel"
            >
              {t("common.cancel")}
            </Button>
            <Button
              type="button"
              variant="primary"
              loading={view.submitting}
              onClick={() => handleSubmit()}
              data-testid="mfa-step-up-confirm"
            >
              {t("common.confirm")}
            </Button>
          </>
        }
      >
        <form onSubmit={handleSubmit} className="space-y-3">
          <p className="text-xs leading-relaxed text-foreground">
            {t("common.mfaDialog.stepUpMessage")}
          </p>
          {categoryLabel && (
            <p className="text-xs font-medium text-amber-700 dark:text-amber-400">
              {t("common.mfaDialog.categoryLine", { category: categoryLabel })}
            </p>
          )}
          <FormField
            label={t("common.mfaDialog.placeholder")}
            error={errorText}
            hint={t("common.mfaDialog.backupHint")}
          >
            <Input
              key={view.sessionId}
              autoFocus
              value={code}
              maxLength={8}
              autoComplete="one-time-code"
              disabled={view.submitting}
              placeholder={t("common.mfaDialog.placeholder")}
              data-testid="mfa-step-up-code-input"
              onChange={(e) => setCode(e.target.value.toUpperCase())}
            />
          </FormField>
        </form>
      </Modal>

      <ConfirmDialog
        open={setupPrompt !== null}
        onClose={handleSetupPromptClose}
        onConfirm={handleSetupPromptConfirm}
        title={t("common.mfaDialog.notEnabledTitle")}
        description={t("common.mfaDialog.notEnabledMessage")}
        confirmLabel={t("common.mfaDialog.goToSetup")}
        cancelLabel={t("common.cancel")}
        danger={false}
      />
    </>
  );
}
