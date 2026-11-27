import { useCallback, useEffect, useState } from "react";
import { Copy, RefreshCw, ShieldCheck } from "lucide-react";
import { createAdmin } from "@/api/admin";
import { fetchPasswordPolicy } from "@/api/audit";
import {
  FormField,
  FormModal,
  IconButton,
  Input,
  PasswordInput,
  Select,
  Switch,
  Tooltip,
} from "@/components/ui";
import { useT } from "@/locales";
import type { AdminRole } from "@/types/api/admin";
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

const USERNAME_RE = /^[a-zA-Z][a-zA-Z0-9_-]*$/;
const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

interface CreateFormState {
  username: string;
  display_name: string;
  email: string;
  password: string;
  role: AdminRole;
  force_password_reset: boolean;
  mfa_required: boolean;
}

const INITIAL_FORM: CreateFormState = {
  username: "",
  display_name: "",
  email: "",
  password: "",
  role: "R_VIEWER",
  force_password_reset: true,
  mfa_required: false,
};

export interface UserCreateModalProps {
  open: boolean;
  onClose: () => void;
  onSuccess: () => void;
}

export function UserCreateModal({
  open,
  onClose,
  onSuccess,
}: UserCreateModalProps) {
  const { t } = useT();
  const [form, setForm] = useState<CreateFormState>(INITIAL_FORM);
  const [errors, setErrors] = useState<
    Partial<Record<keyof CreateFormState, string>>
  >({});
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
      setForm((prev) => ({ ...prev, password: generated }));
    } catch {
      const generated = await generatePasswordWithFetch();
      setForm((prev) => ({ ...prev, password: generated }));
    }
  }, []);

  useEffect(() => {
    if (open) {
      setForm(INITIAL_FORM);
      setErrors({});
      void loadPolicyAndGenerate();
    }
  }, [open, loadPolicyAndGenerate]);

  const handleRegenerate = async () => {
    const next = passwordPolicy
      ? generatePasswordFromPolicy(passwordPolicy)
      : await generatePasswordWithFetch();
    setForm((prev) => ({ ...prev, password: next }));
    setErrors((prev) => ({ ...prev, password: undefined }));
  };

  const validate = (): boolean => {
    const nextErrors: Partial<Record<keyof CreateFormState, string>> = {};
    const username = form.username.trim();
    if (!username) {
      nextErrors.username = t(
        "userManagement.createDialog.validation.usernameRequired",
      );
    } else if (username.length < 3 || username.length > 50) {
      nextErrors.username = t(
        "userManagement.createDialog.validation.usernameLength",
      );
    } else if (!USERNAME_RE.test(username)) {
      nextErrors.username = t(
        "userManagement.createDialog.validation.usernamePattern",
      );
    }

    const displayName = form.display_name.trim();
    if (!displayName) {
      nextErrors.display_name = t(
        "userManagement.createDialog.validation.displayNameRequired",
      );
    } else if (displayName.length > 100) {
      nextErrors.display_name = t(
        "userManagement.createDialog.validation.displayNameLength",
      );
    }

    const email = form.email.trim();
    if (!email) {
      nextErrors.email = t(
        "userManagement.createDialog.validation.emailRequired",
      );
    } else if (!EMAIL_RE.test(email)) {
      nextErrors.email = t(
        "userManagement.createDialog.validation.emailInvalid",
      );
    }

    if (!form.password) {
      nextErrors.password = t(
        "userManagement.createDialog.validation.passwordRequired",
      );
    } else {
      const pwErr = validatePasswordAgainstPolicy(
        form.password,
        passwordPolicy,
        (key, params) => t(key, params),
      );
      if (pwErr) {
        nextErrors.password = pwErr;
      }
    }

    if (!form.role) {
      nextErrors.role = t(
        "userManagement.createDialog.validation.roleRequired",
      );
    }

    setErrors(nextErrors);
    return Object.keys(nextErrors).length === 0;
  };

  const handleSubmit = async () => {
    if (submitting || !validate()) return;
    setSubmitting(true);
    try {
      await createAdmin(
        {
          username: form.username.trim(),
          display_name: form.display_name.trim(),
          email: form.email.trim(),
          password: form.password,
          role: form.role,
          force_password_reset: form.force_password_reset,
          mfa_required: form.mfa_required,
        },
        { showSuccessMessage: false },
      );
      showSuccessToast(t("userManagement.createDialog.messages.createSuccess"));
      onClose();
      onSuccess();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to create user:", error);
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
      title={t("userManagement.createDialog.title")}
      size="md"
      submitLabel={t("common.confirm")}
      cancelLabel={t("common.cancel")}
      submitting={submitting}
      testId="user-create-modal"
    >
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-3.5">
        <FormField
          label={t("userManagement.createDialog.fields.username")}
          htmlFor="create-user-username"
          required
          error={errors.username}
        >
          <Input
            id="create-user-username"
            value={form.username}
            onChange={(e) => {
              setForm((prev) => ({ ...prev, username: e.target.value }));
              setErrors((prev) => ({ ...prev, username: undefined }));
            }}
            placeholder="ops_alice"
            autoComplete="off"
          />
        </FormField>

        <FormField
          label={t("userManagement.createDialog.fields.displayName")}
          htmlFor="create-user-display-name"
          required
          error={errors.display_name}
        >
          <Input
            id="create-user-display-name"
            value={form.display_name}
            onChange={(e) => {
              setForm((prev) => ({ ...prev, display_name: e.target.value }));
              setErrors((prev) => ({ ...prev, display_name: undefined }));
            }}
            placeholder="Alice Wang"
          />
        </FormField>
      </div>

      <div className="grid grid-cols-1 sm:grid-cols-2 gap-3.5">
        <FormField
          label={t("userManagement.createDialog.fields.email")}
          htmlFor="create-user-email"
          required
          error={errors.email}
        >
          <Input
            id="create-user-email"
            type="email"
            value={form.email}
            onChange={(e) => {
              setForm((prev) => ({ ...prev, email: e.target.value }));
              setErrors((prev) => ({ ...prev, email: undefined }));
            }}
            placeholder="alice@example.com"
          />
        </FormField>

        <FormField
          label={t("userManagement.createDialog.fields.role")}
          htmlFor="create-user-role"
          required
          error={errors.role}
        >
          <Select
            id="create-user-role"
            value={form.role}
            onChange={(e) => {
              setForm((prev) => ({
                ...prev,
                role: e.target.value as AdminRole,
              }));
              setErrors((prev) => ({ ...prev, role: undefined }));
            }}
            className="w-full"
            options={[
              {
                value: "R_FINANCE",
                label: t("userManagement.roles.financeWithScope"),
                text: t("userManagement.roles.financeWithScope"),
              },
              {
                value: "R_VIEWER",
                label: t("userManagement.roles.viewerWithScope"),
                text: t("userManagement.roles.viewerWithScope"),
              },
            ]}
          />
        </FormField>
      </div>

      <FormField
        label={t("userManagement.createDialog.fields.password")}
        htmlFor="create-user-password"
        required
        error={errors.password}
        hint={t("userManagement.createDialog.tips.passwordGenerated")}
      >
        <div className="flex items-center gap-1.5">
          <div className="flex-1 min-w-0">
            <PasswordInput
              id="create-user-password"
              value={form.password}
              onChange={(e) => {
                setForm((prev) => ({ ...prev, password: e.target.value }));
                setErrors((prev) => ({ ...prev, password: undefined }));
              }}
              autoComplete="new-password"
            />
          </div>
          <Tooltip
            content={t(
              "userManagement.createDialog.actions.regeneratePassword",
            )}
          >
            <IconButton
              aria-label={t(
                "userManagement.createDialog.actions.regeneratePassword",
              )}
              onClick={() => void handleRegenerate()}
              className="border border-border bg-card"
            >
              <RefreshCw className="h-3.5 w-3.5" />
            </IconButton>
          </Tooltip>
          <Tooltip
            content={t("userManagement.createDialog.actions.copyPassword")}
          >
            <IconButton
              disabled={!form.password}
              aria-label={t("userManagement.createDialog.actions.copyPassword")}
              onClick={() => {
                if (form.password) void copyWithFeedback(form.password);
              }}
              className="border border-border bg-card"
            >
              <Copy className="h-3.5 w-3.5" />
            </IconButton>
          </Tooltip>
        </div>
      </FormField>

      <div className="rounded-lg border border-border divide-y divide-border bg-card">
        <div className="flex items-center justify-between gap-4 px-3.5 py-2.5">
          <div className="min-w-0 space-y-0.5">
            <div className="text-xs font-medium text-foreground">
              {t("userManagement.createDialog.fields.firstLogin")}
            </div>
            <div className="text-[11px] text-muted-foreground">
              {t("userManagement.createDialog.tips.forcePasswordReset")}
            </div>
          </div>
          <Switch
            checked={form.force_password_reset}
            onCheckedChange={(checked) =>
              setForm((prev) => ({ ...prev, force_password_reset: checked }))
            }
            aria-label={t("userManagement.createDialog.fields.firstLogin")}
          />
        </div>
        <div className="flex items-center justify-between gap-4 px-3.5 py-2.5">
          <div className="min-w-0 space-y-0.5">
            <div className="text-xs font-medium text-foreground">
              {t("userManagement.createDialog.fields.mfaRequired")}
            </div>
            <div className="text-[11px] text-muted-foreground">
              {t("userManagement.createDialog.tips.mfaRequired")}
            </div>
          </div>
          <Switch
            checked={form.mfa_required}
            onCheckedChange={(checked) =>
              setForm((prev) => ({ ...prev, mfa_required: checked }))
            }
            aria-label={t("userManagement.createDialog.fields.mfaRequired")}
          />
        </div>
      </div>

      <div className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
        <ShieldCheck className="h-3.5 w-3.5 text-primary shrink-0" />
        <span>{t("common.mfaDialog.title")}</span>
      </div>
    </FormModal>
  );
}
