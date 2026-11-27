import { useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";
import { createTenant } from "@/api/tenant";
import { FormField, FormModal, IconButton, Input } from "@/components/ui";
import { useT } from "@/locales";
import type { CreateTenantRequest } from "@/types/api/tenant";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { logger } from "@/utils/sys/logger";

export interface TenantCreateModalProps {
  open: boolean;
  onClose: () => void;
  onSuccess: () => void;
}

function generateTenantCode(): string {
  const chars = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
  let code = "";
  for (let i = 0; i < 8; i++) {
    code += chars.charAt(Math.floor(Math.random() * chars.length));
  }
  return code;
}

interface TenantCreateErrors {
  tenant_code?: string;
  tenant_id?: string;
  name?: string;
  contact_person?: string;
  contact_phone?: string;
  contact_email?: string;
}

const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
const PHONE_RE = /^1[3-9]\d{9}$/;
const CODE_RE = /^[A-Z0-9]{8}$/;
const ACCOUNT_ID_RE = /^[a-z0-9_-]+$/;

export function TenantCreateModal({
  open,
  onClose,
  onSuccess,
}: TenantCreateModalProps) {
  const { t } = useT();
  const [submitting, setSubmitting] = useState(false);
  const [form, setForm] = useState<CreateTenantRequest>({
    tenant_code: "",
    tenant_id: "",
    name: "",
    contact_person: "",
    contact_phone: "",
    contact_email: "",
  });
  const [errors, setErrors] = useState<TenantCreateErrors>({});

  useEffect(() => {
    if (open) {
      setForm({
        tenant_code: generateTenantCode(),
        tenant_id: "",
        name: "",
        contact_person: "",
        contact_phone: "",
        contact_email: "",
      });
      setErrors({});
    }
  }, [open]);

  const validate = (): boolean => {
    const next: TenantCreateErrors = {};
    const code = (form.tenant_code ?? "").trim();
    const accountId = (form.tenant_id ?? "").trim();
    const name = (form.name ?? "").trim();
    const contactPerson = (form.contact_person ?? "").trim();
    const contactPhone = (form.contact_phone ?? "").trim();
    const contactEmail = (form.contact_email ?? "").trim();

    if (!code) {
      next.tenant_code = t("tenant.createDialog.validation.codeRequired");
    } else if (code.length !== 8) {
      next.tenant_code = t("tenant.createDialog.validation.codeLength");
    } else if (!CODE_RE.test(code)) {
      next.tenant_code = t("tenant.createDialog.validation.codePattern");
    }

    if (!accountId) {
      next.tenant_id = t("tenant.createDialog.validation.accountIdRequired");
    } else if (!ACCOUNT_ID_RE.test(accountId)) {
      next.tenant_id = t("tenant.createDialog.validation.accountIdPattern");
    }

    if (!name) {
      next.name = t("tenant.createDialog.validation.nameRequired");
    } else if (name.length < 2 || name.length > 100) {
      next.name = t("tenant.createDialog.validation.nameLength");
    }

    if (!contactPerson) {
      next.contact_person = t(
        "tenant.createDialog.validation.contactPersonRequired",
      );
    } else if (contactPerson.length > 50) {
      next.contact_person = t(
        "tenant.createDialog.validation.contactPersonLength",
      );
    }

    if (!contactPhone) {
      next.contact_phone = t(
        "tenant.createDialog.validation.contactPhoneRequired",
      );
    } else if (!PHONE_RE.test(contactPhone)) {
      next.contact_phone = t(
        "tenant.createDialog.validation.contactPhonePattern",
      );
    }

    if (!contactEmail) {
      next.contact_email = t(
        "tenant.createDialog.validation.contactEmailRequired",
      );
    } else if (!EMAIL_RE.test(contactEmail)) {
      next.contact_email = t(
        "tenant.createDialog.validation.contactEmailPattern",
      );
    }

    setErrors(next);
    return Object.keys(next).length === 0;
  };

  const handleSubmit = async () => {
    if (submitting || !validate()) return;
    setSubmitting(true);
    try {
      await createTenant({
        tenant_code: form.tenant_code.trim().toUpperCase(),
        tenant_id: form.tenant_id.trim().toLowerCase(),
        name: form.name.trim(),
        contact_person: (form.contact_person ?? "").trim(),
        contact_phone: (form.contact_phone ?? "").trim(),
        contact_email: (form.contact_email ?? "").trim(),
      });
      onSuccess();
      onClose();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to create tenant:", error);
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
      title={t("tenant.createDialog.title")}
      size="md"
      submitLabel={t("common.confirm")}
      submitting={submitting}
      testId="tenant-create-modal"
    >
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <FormField
          label={t("tenant.fields.code")}
          htmlFor="create-tenant-code"
          required
          error={errors.tenant_code}
          hint={t("tenant.createDialog.tips.code")}
        >
          <div className="flex items-center gap-1.5">
            <Input
              id="create-tenant-code"
              maxLength={8}
              value={form.tenant_code}
              onChange={(e) => {
                setForm((prev) => ({
                  ...prev,
                  tenant_code: e.target.value.toUpperCase(),
                }));
                if (errors.tenant_code) {
                  setErrors((prev) => ({ ...prev, tenant_code: undefined }));
                }
              }}
              placeholder={t("tenant.createDialog.placeholders.code")}
              className="font-mono-tech"
            />
            <IconButton
              size="md"
              onClick={() =>
                setForm((prev) => ({
                  ...prev,
                  tenant_code: generateTenantCode(),
                }))
              }
              aria-label={t("common.actions.refresh")}
              title={t("common.actions.refresh")}
              className="border border-border bg-card"
            >
              <RefreshCw className="h-3.5 w-3.5" />
            </IconButton>
          </div>
        </FormField>

        <FormField
          label={t("tenant.fields.accountId")}
          htmlFor="create-tenant-account-id"
          required
          error={errors.tenant_id}
          hint={t("tenant.createDialog.tips.accountId")}
        >
          <Input
            id="create-tenant-account-id"
            value={form.tenant_id}
            onChange={(e) => {
              setForm((prev) => ({
                ...prev,
                tenant_id: e.target.value.toLowerCase(),
              }));
              if (errors.tenant_id) {
                setErrors((prev) => ({ ...prev, tenant_id: undefined }));
              }
            }}
            placeholder={t("tenant.createDialog.placeholders.accountId")}
            className="font-mono-tech"
          />
        </FormField>
      </div>

      <FormField
        label={t("common.fields.name")}
        htmlFor="create-tenant-name"
        required
        error={errors.name}
      >
        <Input
          id="create-tenant-name"
          value={form.name}
          onChange={(e) => {
            setForm((prev) => ({ ...prev, name: e.target.value }));
            if (errors.name) {
              setErrors((prev) => ({ ...prev, name: undefined }));
            }
          }}
          placeholder={t("tenant.createDialog.placeholders.name")}
        />
      </FormField>

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <FormField
          label={t("tenant.fields.contactPerson")}
          htmlFor="create-tenant-contact-person"
          required
          error={errors.contact_person}
        >
          <Input
            id="create-tenant-contact-person"
            value={form.contact_person ?? ""}
            onChange={(e) => {
              setForm((prev) => ({ ...prev, contact_person: e.target.value }));
              if (errors.contact_person) {
                setErrors((prev) => ({ ...prev, contact_person: undefined }));
              }
            }}
            placeholder={t("tenant.createDialog.placeholders.contactPerson")}
          />
        </FormField>

        <FormField
          label={t("tenant.fields.contactPhone")}
          htmlFor="create-tenant-contact-phone"
          required
          error={errors.contact_phone}
        >
          <Input
            id="create-tenant-contact-phone"
            value={form.contact_phone ?? ""}
            onChange={(e) => {
              setForm((prev) => ({ ...prev, contact_phone: e.target.value }));
              if (errors.contact_phone) {
                setErrors((prev) => ({ ...prev, contact_phone: undefined }));
              }
            }}
            placeholder={t("tenant.createDialog.placeholders.contactPhone")}
            className="font-mono-tech"
          />
        </FormField>
      </div>

      <FormField
        label={t("tenant.fields.contactEmail")}
        htmlFor="create-tenant-contact-email"
        required
        error={errors.contact_email}
      >
        <Input
          id="create-tenant-contact-email"
          type="email"
          value={form.contact_email ?? ""}
          onChange={(e) => {
            setForm((prev) => ({ ...prev, contact_email: e.target.value }));
            if (errors.contact_email) {
              setErrors((prev) => ({ ...prev, contact_email: undefined }));
            }
          }}
          placeholder={t("tenant.createDialog.placeholders.contactEmail")}
        />
      </FormField>
    </FormModal>
  );
}
