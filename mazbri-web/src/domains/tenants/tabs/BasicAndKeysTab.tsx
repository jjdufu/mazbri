import { useCallback, useEffect, useRef, useState } from "react";
import {
  AlertTriangle,
  Building2,
  Copy,
  Download,
  KeyRound,
  Pencil,
  Plus,
} from "lucide-react";
import { updateApiKey as updateApiKeyApi } from "@/api/api-key";
import {
  createApiKey,
  deleteApiKey,
  fetchTenantApiKeys,
  rotateApiKey,
  setTenantEnabled,
  updateTenant,
} from "@/api/tenant";
import {
  Button,
  ConfirmDialog,
  CopyableText,
  EmptyState,
  EntityDetailCard,
  FormField,
  FormModal,
  Input,
  Modal,
  NumberInput,
  StatusBadge,
  Switch,
  Textarea,
} from "@/components/ui";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import type {
  ApiKey,
  CreateApiKeyRequest,
  UpdateApiKeyRequest,
} from "@/types/api/api-key";
import type { Tenant, UpdateTenantRequest } from "@/types/api/tenant";
import { EMPTY_PLACEHOLDER, formatDateTime } from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { isBizRole } from "@/utils/ops/roles";
import { logger } from "@/utils/sys/logger";
import { copyWithFeedback } from "@/utils/ui/copy-feedback";
import {
  showErrorToast,
  showSuccessToast,
  showWarningToast,
} from "@/utils/ui/message-guard";

export interface BasicAndKeysTabProps {
  tenantCode: string;
  tenant: Tenant | null;
  loading: boolean;
  onRefreshTenant: () => void;
}

const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

export function BasicAndKeysTab({
  tenantCode,
  tenant,
  loading,
  onRefreshTenant,
}: BasicAndKeysTabProps) {
  const { t } = useT();
  const role = useAuthStore((s) => s.info.role);
  const canWrite = isBizRole(role);

  // Basic Info Edit state
  const [editOpen, setEditOpen] = useState(false);
  const [savingBasic, setSavingBasic] = useState(false);
  const [editForm, setEditForm] = useState<UpdateTenantRequest>({
    name: "",
    contact_person: "",
    contact_phone: "",
    contact_email: "",
  });
  const [editErrors, setEditErrors] = useState<{
    name?: string;
    contact_email?: string;
  }>({});

  // Status Switch confirm state
  const [pendingStatus, setPendingStatus] = useState<boolean | null>(null);
  const [statusSubmitting, setStatusSubmitting] = useState(false);

  // API Keys state
  const [keysLoading, setKeysLoading] = useState(false);
  const [apiKeys, setApiKeys] = useState<ApiKey[]>([]);
  const [apiKeyActions, setApiKeyActions] = useState<{
    create: boolean;
    rotate?: boolean;
    delete: boolean;
  }>({ create: false, rotate: false, delete: false });

  const [createKeyOpen, setCreateKeyOpen] = useState(false);
  const [createDays, setCreateDays] = useState(365);
  const [creatingKey, setCreatingKey] = useState(false);

  const [renewTarget, setRenewTarget] = useState<ApiKey | null>(null);
  const [renewDays, setRenewDays] = useState(365);
  const [renewingKey, setRenewingKey] = useState(false);

  const [rotateTarget, setRotateTarget] = useState<ApiKey | null>(null);
  const [rotatingPrefix, setRotatingPrefix] = useState<string | null>(null);

  const [revokeTarget, setRevokeTarget] = useState<ApiKey | null>(null);
  const [revokingKey, setRevokingKey] = useState(false);

  const [secretModal, setSecretModal] = useState<{
    open: boolean;
    mode: "create" | "rotate";
    value: string;
  }>({ open: false, mode: "create", value: "" });

  const keysAbortRef = useRef<AbortController | null>(null);

  const loadApiKeys = useCallback(async () => {
    if (!tenantCode || !canWrite) return;
    keysAbortRef.current?.abort();
    const controller = new AbortController();
    keysAbortRef.current = controller;
    setKeysLoading(true);
    try {
      const res = await fetchTenantApiKeys(tenantCode, controller.signal);
      if (controller.signal.aborted) return;
      setApiKeys(res.keys ?? []);
      if (res.actions) {
        setApiKeyActions(res.actions);
      }
    } catch (error) {
      if (!controller.signal.aborted && !isMfaStepUpCancelled(error)) {
        logger.error("Failed to load API keys:", error);
      }
    } finally {
      if (!controller.signal.aborted) {
        setKeysLoading(false);
      }
    }
  }, [tenantCode, canWrite]);

  useEffect(() => {
    void loadApiKeys();
    return () => keysAbortRef.current?.abort();
  }, [loadApiKeys]);

  const handleOpenEdit = () => {
    if (!tenant) return;
    setEditForm({
      name: tenant.name,
      contact_person: tenant.contact_person ?? "",
      contact_phone: tenant.contact_phone ?? "",
      contact_email: tenant.contact_email ?? "",
    });
    setEditErrors({});
    setEditOpen(true);
  };

  const handleSaveBasic = async () => {
    if (!tenant || savingBasic) return;
    const nextErrors: { name?: string; contact_email?: string } = {};
    const name = (editForm.name ?? "").trim();
    const email = (editForm.contact_email ?? "").trim();
    if (!name) {
      nextErrors.name = t("tenant.detail.validation.nameRequired");
    } else if (name.length < 2 || name.length > 100) {
      nextErrors.name = t("tenant.detail.validation.nameLength");
    }
    if (email && !EMAIL_RE.test(email)) {
      nextErrors.contact_email = t("tenant.detail.validation.email");
    }
    setEditErrors(nextErrors);
    if (Object.keys(nextErrors).length > 0) return;

    setSavingBasic(true);
    try {
      await updateTenant(tenant.tenant_code, {
        name,
        contact_person: (editForm.contact_person ?? "").trim(),
        contact_phone: (editForm.contact_phone ?? "").trim(),
        contact_email: email,
      });
      showSuccessToast(t("common.feedback.saveSuccess"));
      setEditOpen(false);
      onRefreshTenant();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to update tenant:", error);
      }
    } finally {
      setSavingBasic(false);
    }
  };

  const handleConfirmStatus = async () => {
    if (!tenant || pendingStatus === null || statusSubmitting) return;
    setStatusSubmitting(true);
    try {
      await setTenantEnabled(tenant.tenant_code, { enabled: pendingStatus });
      showSuccessToast(
        pendingStatus
          ? t("common.feedback.enabled")
          : t("common.feedback.disabled"),
      );
      setPendingStatus(null);
      onRefreshTenant();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to change tenant status:", error);
      }
      setPendingStatus(null);
    } finally {
      setStatusSubmitting(false);
    }
  };

  const handleCreateKey = async () => {
    if (creatingKey) return;
    if (!Number.isFinite(createDays) || createDays < 1 || createDays > 3650) {
      showWarningToast(t("tenant.apiKey.validation.renewDaysRange"));
      return;
    }
    setCreatingKey(true);
    try {
      const payload: CreateApiKeyRequest = {
        scopes: ["api:read", "api:write"],
        expires_days: createDays,
      };
      const res = await createApiKey(tenantCode, payload);
      showSuccessToast(t("tenant.messages.apiKeyCreated"));
      setCreateKeyOpen(false);
      setSecretModal({ open: true, mode: "create", value: res.api_key });
      void loadApiKeys();
    } catch (error: unknown) {
      if (isMfaStepUpCancelled(error)) return;
      const apiError = error as {
        data?: { error?: string };
        response?: { data?: { error?: string } };
      };
      const code =
        apiError?.data?.error ?? apiError?.response?.data?.error ?? "";
      if (code === "api_key_already_exists") {
        showWarningToast(t("tenant.messages.apiKeyExists"));
      } else {
        logger.error("Failed to create API key:", error);
      }
    } finally {
      setCreatingKey(false);
    }
  };

  const handleRenewKey = async () => {
    if (!renewTarget || renewingKey) return;
    if (!Number.isFinite(renewDays) || renewDays < 1 || renewDays > 3650) {
      showWarningToast(t("tenant.apiKey.validation.renewDaysRange"));
      return;
    }
    setRenewingKey(true);
    try {
      const payload: UpdateApiKeyRequest = {
        expires_days: renewDays,
      };
      await updateApiKeyApi(tenantCode, renewTarget.key_prefix, payload);
      showSuccessToast(t("tenant.messages.apiKeyRenewed"));
      setRenewTarget(null);
      void loadApiKeys();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to renew API key:", error);
      }
    } finally {
      setRenewingKey(false);
    }
  };

  const handleConfirmRotate = async () => {
    if (!rotateTarget || rotatingPrefix) return;
    const prefix = rotateTarget.key_prefix;
    setRotatingPrefix(prefix);
    try {
      const res = await rotateApiKey(tenantCode, prefix);
      showSuccessToast(t("tenant.messages.apiKeyRotated"));
      setRotateTarget(null);
      setSecretModal({ open: true, mode: "rotate", value: res.api_key });
      await loadApiKeys();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to rotate API key:", error);
      }
    } finally {
      setRotatingPrefix(null);
    }
  };

  const handleConfirmRevoke = async () => {
    if (!revokeTarget || revokingKey) return;
    setRevokingKey(true);
    try {
      await deleteApiKey(tenantCode, revokeTarget.key_prefix);
      showSuccessToast(t("tenant.messages.apiKeyRevoked"));
      setRevokeTarget(null);
      void loadApiKeys();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to revoke API key:", error);
      }
    } finally {
      setRevokingKey(false);
    }
  };

  const handleDownloadKey = (format: "txt" | "json") => {
    if (!secretModal.value) {
      showErrorToast(t("tenant.messages.apiKeyMissing"));
      return;
    }
    try {
      const isJson = format === "json";
      const content = isJson
        ? `${JSON.stringify(
            {
              tenant_code: tenantCode,
              api_key: secretModal.value,
              note: t("tenant.apiKey.downloadNote"),
            },
            null,
            2,
          )}\n`
        : `Tenant Code: ${tenantCode}\nAPI Key: ${secretModal.value}\n\n${t("tenant.apiKey.downloadNote")}\n`;
      const blob = new Blob([content], {
        type: isJson
          ? "application/json;charset=utf-8"
          : "text/plain;charset=utf-8",
      });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = `api_key_${tenantCode}.${format}`;
      document.body.appendChild(link);
      link.click();
      document.body.removeChild(link);
      URL.revokeObjectURL(url);
      showSuccessToast(t("tenant.messages.apiKeyDownloaded"));
    } catch (error) {
      logger.error("Failed to download API key:", error);
      showErrorToast(t("tenant.messages.apiKeyDownloadFailed"));
    }
  };

  const infoItems = [
    {
      label: t("tenant.fields.code"),
      value: tenant?.tenant_code ? (
        <CopyableText value={tenant.tenant_code} mono />
      ) : (
        EMPTY_PLACEHOLDER
      ),
    },
    {
      label: t("tenant.fields.accountId"),
      value: tenant?.tenant_id ? (
        <CopyableText value={tenant.tenant_id} mono />
      ) : (
        EMPTY_PLACEHOLDER
      ),
    },
    {
      label: t("common.fields.name"),
      value: tenant?.name || EMPTY_PLACEHOLDER,
    },
    {
      label: t("tenant.fields.contactPerson"),
      value: tenant?.contact_person || EMPTY_PLACEHOLDER,
    },
    {
      label: t("tenant.fields.contactPhone"),
      value: (
        <span className="font-mono-tech">
          {tenant?.contact_phone || EMPTY_PLACEHOLDER}
        </span>
      ),
    },
    {
      label: t("tenant.fields.contactEmail"),
      value: tenant?.contact_email || EMPTY_PLACEHOLDER,
    },
    {
      label: t("common.fields.status"),
      value: (
        <div className="inline-flex items-center gap-2">
          <Switch
            checked={tenant?.enabled ?? false}
            disabled={!canWrite || statusSubmitting || loading}
            onCheckedChange={(next) => setPendingStatus(next)}
            aria-label={t("common.fields.status")}
          />
          <StatusBadge
            status={tenant?.enabled ? "ENABLED" : "DISABLED"}
            label={
              tenant?.enabled
                ? t("common.status.enabled")
                : t("common.status.disabled")
            }
          />
        </div>
      ),
    },
    {
      label: t("common.fields.createdAt"),
      value: (
        <span className="font-mono-tech">
          {formatDateTime(tenant?.created_at)}
        </span>
      ),
    },
    {
      label: t("common.fields.updatedAt"),
      value: (
        <span className="font-mono-tech">
          {formatDateTime(tenant?.updated_at)}
        </span>
      ),
    },
  ];

  return (
    <div className="space-y-4 min-w-0">
      <EntityDetailCard
        title={t("tenant.detail.basicInfo.title")}
        icon={<Building2 className="h-4 w-4" />}
        testId="tenant-basic-info-card"
        actions={
          canWrite ? (
            <Button
              type="button"
              variant="primary"
              size="sm"
              disabled={!tenant || loading}
              onClick={handleOpenEdit}
              data-testid="tenant-basic-edit-btn"
            >
              <Pencil className="h-3.5 w-3.5" />
              {t("common.actions.modify")}
            </Button>
          ) : undefined
        }
      >
        <div className="grid grid-cols-1 gap-px overflow-hidden rounded-md border border-border bg-border md:grid-cols-2">
          {infoItems.map((item, idx) => (
            <div
              key={idx}
              className="flex flex-col gap-1 bg-card px-3.5 py-2.5 sm:flex-row sm:items-center sm:justify-between sm:gap-3"
            >
              <span className="shrink-0 text-xs text-muted-foreground">
                {item.label}
              </span>
              <span className="min-w-0 text-xs font-medium text-foreground sm:text-right">
                {item.value}
              </span>
            </div>
          ))}
        </div>
      </EntityDetailCard>

      {canWrite && (
        <EntityDetailCard
          title={t("tenant.detail.tabs.apiKeys")}
          icon={<KeyRound className="h-4 w-4" />}
          testId="tenant-api-keys-card"
          bodyClassName="p-0"
          actions={
            apiKeyActions.create ? (
              <Button
                type="button"
                variant="primary"
                size="sm"
                onClick={() => {
                  setCreateDays(365);
                  setCreateKeyOpen(true);
                }}
                data-testid="tenant-add-apikey-btn"
              >
                <Plus className="h-3.5 w-3.5" />
                {t("tenant.actions.addApiKey")}
              </Button>
            ) : undefined
          }
        >
          {keysLoading && apiKeys.length === 0 ? (
            <div className="py-10 text-center text-xs text-muted-foreground">
              {t("layout.loading")}
            </div>
          ) : apiKeys.length === 0 ? (
            <div className="p-4">
              <EmptyState title={t("common.feedback.noData")} />
            </div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full border-collapse text-xs text-foreground">
                <thead>
                  <tr className="border-b border-border bg-muted/45 text-muted-foreground">
                    <th className="h-9 px-4 text-left font-medium whitespace-nowrap">
                      {t("tenant.apiKey.key")}
                    </th>
                    <th className="h-9 px-3 text-left font-medium whitespace-nowrap">
                      {t("common.fields.status")}
                    </th>
                    <th className="h-9 px-3 text-left font-medium whitespace-nowrap">
                      {t("tenant.apiKey.expiry")}
                    </th>
                    <th className="h-9 px-4 text-right font-medium whitespace-nowrap">
                      {t("common.fields.operations")}
                    </th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-border-subtle">
                  {apiKeys.map((row) => (
                    <tr
                      key={row.key_prefix}
                      className="transition-colors hover:bg-hover-bg/75"
                    >
                      <td className="px-4 py-2.5">
                        <code className="rounded bg-muted px-2 py-0.5 font-mono-tech text-xs text-foreground">
                          ****{row.key_suffix}
                        </code>
                      </td>
                      <td className="px-3 py-2.5">
                        <StatusBadge
                          status={row.is_active ? "ENABLED" : "DISABLED"}
                          label={
                            row.is_active
                              ? t("common.status.enabled")
                              : t("common.status.disabled")
                          }
                        />
                      </td>
                      <td className="px-3 py-2.5 font-mono-tech">
                        {row.expires_at
                          ? formatDateTime(row.expires_at)
                          : t("common.status.neverExpires")}
                      </td>
                      <td className="px-4 py-2.5 text-right">
                        <div className="inline-flex items-center justify-end gap-1">
                          {apiKeyActions.rotate !== false && (
                            <Button
                              type="button"
                              variant="link"
                              size="xs"
                              loading={rotatingPrefix === row.key_prefix}
                              onClick={() => setRotateTarget(row)}
                            >
                              {t("tenant.actions.rotateApiKey")}
                            </Button>
                          )}
                          <Button
                            type="button"
                            variant="link"
                            size="xs"
                            onClick={() => {
                              setRenewTarget(row);
                              setRenewDays(365);
                            }}
                          >
                            {t("common.actions.renew")}
                          </Button>
                          {apiKeyActions.delete && (
                            <Button
                              type="button"
                              variant="link"
                              size="xs"
                              className="text-danger-fg hover:text-danger-fg"
                              onClick={() => setRevokeTarget(row)}
                            >
                              {t("tenant.actions.revokeApiKey")}
                            </Button>
                          )}
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </EntityDetailCard>
      )}

      {/* Edit Basic Info Modal */}
      <FormModal
        open={editOpen}
        onClose={() => setEditOpen(false)}
        onSubmit={handleSaveBasic}
        title={t("tenant.detail.basicInfo.editTitle")}
        size="md"
        submitting={savingBasic}
        testId="tenant-basic-edit-modal"
      >
        <FormField
          label={t("common.fields.name")}
          htmlFor="edit-tenant-name"
          required
          error={editErrors.name}
        >
          <Input
            id="edit-tenant-name"
            value={editForm.name ?? ""}
            onChange={(e) => {
              setEditForm((prev) => ({ ...prev, name: e.target.value }));
              if (editErrors.name) {
                setEditErrors((prev) => ({ ...prev, name: undefined }));
              }
            }}
          />
        </FormField>

        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <FormField
            label={t("tenant.fields.contactPerson")}
            htmlFor="edit-tenant-contact-person"
          >
            <Input
              id="edit-tenant-contact-person"
              value={editForm.contact_person ?? ""}
              onChange={(e) =>
                setEditForm((prev) => ({
                  ...prev,
                  contact_person: e.target.value,
                }))
              }
            />
          </FormField>

          <FormField
            label={t("tenant.fields.contactPhone")}
            htmlFor="edit-tenant-contact-phone"
          >
            <Input
              id="edit-tenant-contact-phone"
              value={editForm.contact_phone ?? ""}
              onChange={(e) =>
                setEditForm((prev) => ({
                  ...prev,
                  contact_phone: e.target.value,
                }))
              }
              className="font-mono-tech"
            />
          </FormField>
        </div>

        <FormField
          label={t("tenant.fields.contactEmail")}
          htmlFor="edit-tenant-contact-email"
          error={editErrors.contact_email}
        >
          <Input
            id="edit-tenant-contact-email"
            type="email"
            value={editForm.contact_email ?? ""}
            onChange={(e) => {
              setEditForm((prev) => ({
                ...prev,
                contact_email: e.target.value,
              }));
              if (editErrors.contact_email) {
                setEditErrors((prev) => ({
                  ...prev,
                  contact_email: undefined,
                }));
              }
            }}
          />
        </FormField>
      </FormModal>

      {/* Status Confirm */}
      <ConfirmDialog
        open={pendingStatus !== null}
        onClose={() => setPendingStatus(null)}
        onConfirm={() => void handleConfirmStatus()}
        loading={statusSubmitting}
        danger={pendingStatus === false}
        title={t("common.tips")}
        confirmLabel={t("common.confirm")}
        description={
          tenant && pendingStatus !== null
            ? pendingStatus
              ? t("tenant.messages.enableTenantConfirm", { name: tenant.name })
              : t("tenant.messages.disableTenantConfirm", { name: tenant.name })
            : ""
        }
      />

      {/* Create API Key Modal */}
      <FormModal
        open={createKeyOpen}
        onClose={() => setCreateKeyOpen(false)}
        onSubmit={handleCreateKey}
        title={t("tenant.apiKey.createTitle")}
        size="sm"
        submitLabel={t("common.confirm")}
        submitting={creatingKey}
        testId="tenant-create-apikey-modal"
      >
        <FormField label={t("tenant.apiKey.expiryDays")} required>
          <div className="flex items-center gap-2">
            <NumberInput
              value={createDays}
              onChange={setCreateDays}
              min={1}
              max={3650}
              step={30}
              className="w-44"
              aria-label={t("tenant.apiKey.expiryDays")}
            />
            <span className="shrink-0 whitespace-nowrap text-xs text-muted-foreground">
              {t("common.units.days")} ({t("common.defaults.oneYear")})
            </span>
          </div>
        </FormField>
      </FormModal>

      {/* Renew API Key Modal */}
      <FormModal
        open={Boolean(renewTarget)}
        onClose={() => setRenewTarget(null)}
        onSubmit={handleRenewKey}
        title={t("tenant.actions.renewApiKey")}
        size="sm"
        submitLabel={t("common.confirm")}
        submitting={renewingKey}
        testId="tenant-renew-apikey-modal"
      >
        <FormField label={t("tenant.apiKey.key")}>
          <Input
            value={renewTarget?.display_key ?? ""}
            disabled
            className="font-mono-tech"
          />
        </FormField>
        <FormField label={t("tenant.apiKey.currentExpiry")}>
          <Input
            value={
              renewTarget?.expires_at
                ? formatDateTime(renewTarget.expires_at)
                : t("common.status.neverExpires")
            }
            disabled
          />
        </FormField>
        <FormField label={t("tenant.apiKey.renewDays")} required>
          <div className="flex items-center gap-2">
            <NumberInput
              value={renewDays}
              onChange={setRenewDays}
              min={1}
              max={3650}
              step={30}
              className="w-44"
              aria-label={t("tenant.apiKey.renewDays")}
            />
            <span className="shrink-0 whitespace-nowrap text-xs text-muted-foreground">
              {t("common.units.days")}
            </span>
          </div>
        </FormField>
      </FormModal>

      {/* Rotate Confirm */}
      <ConfirmDialog
        open={Boolean(rotateTarget)}
        onClose={() => setRotateTarget(null)}
        onConfirm={() => void handleConfirmRotate()}
        loading={Boolean(rotatingPrefix)}
        danger={false}
        title={t("common.tips")}
        confirmLabel={t("common.confirm")}
        description={t("tenant.messages.rotateApiKeyConfirm")}
      />

      {/* Revoke Confirm */}
      <ConfirmDialog
        open={Boolean(revokeTarget)}
        onClose={() => setRevokeTarget(null)}
        onConfirm={() => void handleConfirmRevoke()}
        loading={revokingKey}
        danger
        title={t("common.tips")}
        confirmLabel={t("tenant.actions.revokeApiKey")}
        description={t("tenant.messages.revokeApiKeyConfirm")}
      />

      {/* One-time Secret Reveal Modal */}
      <Modal
        open={secretModal.open}
        onClose={() => setSecretModal((prev) => ({ ...prev, open: false }))}
        title={
          secretModal.mode === "rotate"
            ? t("tenant.apiKey.rotateTitle")
            : t("tenant.apiKey.createTitle")
        }
        size="md"
        closeOnBackdrop={false}
        testId="tenant-apikey-secret-modal"
        footer={
          <>
            <Button
              type="button"
              variant="primary"
              onClick={() => void copyWithFeedback(secretModal.value)}
            >
              <Copy className="h-3.5 w-3.5" />
              {t("common.actions.copy")}
            </Button>
            <Button
              type="button"
              variant="outline"
              onClick={() => handleDownloadKey("txt")}
            >
              <Download className="h-3.5 w-3.5" />
              {t("tenant.apiKey.downloadTxt")}
            </Button>
            <Button
              type="button"
              variant="outline"
              onClick={() => handleDownloadKey("json")}
            >
              <Download className="h-3.5 w-3.5" />
              {t("tenant.apiKey.downloadJson")}
            </Button>
            <Button
              type="button"
              variant="outline"
              onClick={() =>
                setSecretModal((prev) => ({ ...prev, open: false }))
              }
            >
              {t("common.actions.close")}
            </Button>
          </>
        }
      >
        <div className="flex items-start gap-2 rounded-md border border-warning-border bg-warning-bg px-3 py-2 text-xs text-warning-fg">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
          <span>{t("tenant.apiKey.secretWarning")}</span>
        </div>
        <div className="space-y-1.5">
          <div className="text-xs font-medium text-foreground">
            {t("tenant.apiKey.secretLabel")}
          </div>
          <Textarea
            rows={3}
            readOnly
            value={secretModal.value}
            className="font-mono-tech"
          />
        </div>
      </Modal>
    </div>
  );
}
