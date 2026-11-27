import { useEffect, useState } from "react";
import { updateAdmin } from "@/api/admin";
import { FormField, FormModal, Input, Select, Switch } from "@/components/ui";
import { useT } from "@/locales";
import type {
  AdminRole,
  AdminUser,
  UpdateAdminRequest,
} from "@/types/api/admin";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { logger } from "@/utils/sys/logger";
import { showSuccessToast } from "@/utils/ui/message-guard";

const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

interface EditFormState {
  display_name: string;
  email: string;
  role: AdminRole;
  force_password_reset: boolean;
  mfa_required: boolean;
}

export interface UserEditModalProps {
  open: boolean;
  admin: AdminUser | null;
  onClose: () => void;
  onSuccess: (updated: AdminUser) => void;
}

export function UserEditModal({
  open,
  admin,
  onClose,
  onSuccess,
}: UserEditModalProps) {
  const { t } = useT();
  const [form, setForm] = useState<EditFormState>({
    display_name: "",
    email: "",
    role: "R_VIEWER",
    force_password_reset: false,
    mfa_required: false,
  });
  const [errors, setErrors] = useState<
    Partial<Record<keyof EditFormState, string>>
  >({});
  const [saving, setSaving] = useState(false);

  const isSuperAdminTarget = admin?.role === "R_ADMIN";

  useEffect(() => {
    if (open && admin) {
      setForm({
        display_name: admin.display_name ?? "",
        email: admin.email ?? "",
        role: admin.role,
        force_password_reset: admin.force_password_reset,
        mfa_required: admin.mfa_required,
      });
      setErrors({});
    }
  }, [open, admin]);

  const validate = (): boolean => {
    const nextErrors: Partial<Record<keyof EditFormState, string>> = {};
    const displayName = form.display_name.trim();
    if (!displayName) {
      nextErrors.display_name = t(
        "userManagement.detail.validation.displayNameRequired",
      );
    } else if (displayName.length > 100) {
      nextErrors.display_name = t(
        "userManagement.detail.validation.displayNameLength",
      );
    }

    const email = form.email.trim();
    if (email && !EMAIL_RE.test(email)) {
      nextErrors.email = t("userManagement.detail.validation.emailInvalid");
    }

    if (!isSuperAdminTarget && !form.role) {
      nextErrors.role = t("userManagement.detail.validation.roleRequired");
    }

    setErrors(nextErrors);
    return Object.keys(nextErrors).length === 0;
  };

  const handleSubmit = async () => {
    if (!admin || saving || !validate()) return;
    const payload: UpdateAdminRequest = {
      display_name: form.display_name.trim(),
      email: form.email.trim() || undefined,
    };
    if (!isSuperAdminTarget) {
      payload.role = form.role;
      payload.mfa_required = form.mfa_required;
      payload.force_password_reset = form.force_password_reset;
    }

    setSaving(true);
    try {
      const updated = await updateAdmin(admin.id, payload);
      showSuccessToast(t("common.feedback.saveSuccess"));
      onClose();
      onSuccess(updated);
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to update admin:", error);
      }
    } finally {
      setSaving(false);
    }
  };

  return (
    <FormModal
      open={open}
      onClose={onClose}
      onSubmit={handleSubmit}
      title={
        isSuperAdminTarget
          ? t("userManagement.detail.editTitle")
          : t("userManagement.detail.editUserTitle")
      }
      size="md"
      submitLabel={t("common.actions.save")}
      cancelLabel={t("common.cancel")}
      submitting={saving}
      testId="user-edit-modal"
    >
      <FormField
        label={t("userManagement.detail.labels.displayName")}
        htmlFor="edit-user-display-name"
        required
        error={errors.display_name}
      >
        <Input
          id="edit-user-display-name"
          value={form.display_name}
          onChange={(e) => {
            setForm((prev) => ({ ...prev, display_name: e.target.value }));
            setErrors((prev) => ({ ...prev, display_name: undefined }));
          }}
          placeholder="Alice Wang"
        />
      </FormField>

      <FormField
        label={t("userManagement.detail.labels.email")}
        htmlFor="edit-user-email"
        error={errors.email}
      >
        <Input
          id="edit-user-email"
          type="email"
          value={form.email}
          onChange={(e) => {
            setForm((prev) => ({ ...prev, email: e.target.value }));
            setErrors((prev) => ({ ...prev, email: undefined }));
          }}
          placeholder="alice@example.com"
        />
      </FormField>

      {!isSuperAdminTarget && (
        <>
          <FormField
            label={t("userManagement.detail.labels.role")}
            htmlFor="edit-user-role"
            required
            error={errors.role}
          >
            <Select
              id="edit-user-role"
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

          <div className="rounded-lg border border-border divide-y divide-border bg-muted/25">
            <div className="flex items-center justify-between gap-4 px-3.5 py-2.5">
              <span className="text-xs font-medium text-foreground">
                {t("userManagement.detail.labels.forcePasswordReset")}
              </span>
              <Switch
                checked={form.force_password_reset}
                onCheckedChange={(checked) =>
                  setForm((prev) => ({
                    ...prev,
                    force_password_reset: checked,
                  }))
                }
                aria-label={t(
                  "userManagement.detail.labels.forcePasswordReset",
                )}
              />
            </div>
            <div className="flex items-center justify-between gap-4 px-3.5 py-2.5">
              <div className="min-w-0 space-y-0.5">
                <div className="text-xs font-medium text-foreground">
                  {t("userManagement.detail.labels.mfaRequired")}
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
                aria-label={t("userManagement.detail.labels.mfaRequired")}
              />
            </div>
          </div>
        </>
      )}
    </FormModal>
  );
}
