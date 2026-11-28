import { useCallback, useEffect, useState } from "react";
import { Copy, RefreshCw } from "lucide-react";
import { resetAdminPassword } from "@/api/admin";
import { fetchPasswordPolicy } from "@/api/audit";
import {
  FormField,
  FormModal,
  IconButton,
  PasswordInput,
  Switch,
  Tooltip,
} from "@/components/ui";
import { useT } from "@/locales";
import type { AdminUser } from "@/types/api/admin";
import type { PasswordPolicy } from "@/types/api/settings";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import {
  generatePasswordFromPolicy,
  generatePasswordWithFetch,
  resolvePasswordPolicyPayload,
  validatePasswordAgainstPolicy,
} from "@/utils/password";
import { logger } from "@/utils/sys/logger";
import { copyWithFeedback } from "@/utils/ui/copy-feedback";
import { showSuccessToast } from "@/utils/ui/message-guard";

export interface UserResetPasswordModalProps {
  open: boolean;
  admin: AdminUser | null;
  onClose: () => void;
  onSuccess: () => void;
}

export function UserResetPasswordModal({
  open,
  admin,
  onClose,
  onSuccess,
}: UserResetPasswordModalProps) {
  const { t } = useT();
  const [newPassword, setNewPassword] = useState("");
  const [forceReset, setForceReset] = useState(true);
  const [error, setError] = useState<string | undefined>(undefined);
  const [passwordPolicy, setPasswordPolicy] = useState<PasswordPolicy | null>(
    null,
  );
  const [submitting, setSubmitting] = useState(false);

  const loadPolicyAndGenerate = useCallback(async () => {
    try {
      const response = await fetchPasswordPolicy();
      const policy = resolvePasswordPolicyPayload(response);
      setPasswordPolicy(policy);
      const generated = policy
        ? generatePasswordFromPolicy(policy)
        : await generatePasswordWithFetch();
      setNewPassword(generated);
    } catch {
      const generated = await generatePasswordWithFetch();
      setNewPassword(generated);
    }
  }, []);

  useEffect(() => {
    if (open) {
      setForceReset(true);
      setError(undefined);
      void loadPolicyAndGenerate();
    } else {
      setNewPassword("");
      setError(undefined);
    }
  }, [open, loadPolicyAndGenerate]);

  const handleRegenerate = async () => {
    const generated = passwordPolicy
      ? generatePasswordFromPolicy(passwordPolicy)
      : await generatePasswordWithFetch();
    setNewPassword(generated);
    setError(undefined);
  };

  const handleSubmit = async () => {
    if (!admin || submitting) return;
    if (!newPassword) {
      setError(
        t("userManagement.resetPasswordDialog.validation.newPasswordRequired"),
      );
      return;
    }
    const policyError = validatePasswordAgainstPolicy(
      newPassword,
      passwordPolicy,
      (key, params) => t(key, params),
    );
    if (policyError) {
      setError(policyError);
      return;
    }

    setSubmitting(true);
    try {
      await resetAdminPassword(admin.id, {
        new_password: newPassword,
        force_password_reset: forceReset,
      });
      showSuccessToast(t("common.feedback.resetPasswordSuccess"));
      onClose();
      onSuccess();
    } catch (err) {
      if (!isMfaStepUpCancelled(err)) {
        logger.error("Failed to reset password:", err);
      }
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <FormModal
      open={open}
      onClose={onClose}
      onSubmit={handleSubmit}
      title={t("userManagement.resetPasswordDialog.title")}
      subtitle={admin?.username}
      size="md"
      submitLabel={t("common.confirm")}
      cancelLabel={t("common.cancel")}
      submitting={submitting}
      testId="user-reset-password-modal"
    >
      <FormField
        label={t("userManagement.resetPasswordDialog.fields.newPassword")}
        htmlFor="reset-user-password"
        required
        error={error}
      >
        <div className="flex items-center gap-1.5">
          <div className="flex-1 min-w-0">
            <PasswordInput
              id="reset-user-password"
              value={newPassword}
              onChange={(e) => {
                setNewPassword(e.target.value);
                setError(undefined);
              }}
              autoComplete="new-password"
            />
          </div>
          <Tooltip
            content={t(
              "userManagement.resetPasswordDialog.actions.regeneratePassword",
            )}
          >
            <IconButton
              aria-label={t(
                "userManagement.resetPasswordDialog.actions.regeneratePassword",
              )}
              onClick={() => void handleRegenerate()}
              className="border border-border bg-card"
            >
              <RefreshCw className="h-3.5 w-3.5" />
            </IconButton>
          </Tooltip>
          <Tooltip
            content={t(
              "userManagement.resetPasswordDialog.actions.copyPassword",
            )}
          >
            <IconButton
              disabled={!newPassword}
              aria-label={t(
                "userManagement.resetPasswordDialog.actions.copyPassword",
              )}
              onClick={() => {
                if (newPassword) void copyWithFeedback(newPassword);
              }}
              className="border border-border bg-card"
            >
              <Copy className="h-3.5 w-3.5" />
            </IconButton>
          </Tooltip>
        </div>
      </FormField>

      <div className="flex items-center justify-between gap-4 rounded-lg border border-border bg-card px-3.5 py-2.5">
        <span className="text-xs font-medium text-foreground">
          {t("userManagement.resetPasswordDialog.forceReset")}
        </span>
        <Switch
          checked={forceReset}
          onCheckedChange={setForceReset}
          aria-label={t("userManagement.resetPasswordDialog.forceReset")}
        />
      </div>
    </FormModal>
  );
}
