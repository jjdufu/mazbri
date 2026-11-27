import { useCallback, useEffect, useRef, useState } from "react";
import {
  AlertTriangle,
  Copy,
  Download,
  KeyRound,
  Plus,
  RefreshCw,
} from "lucide-react";
import { updateApiKey as updateApiKeyApi } from "@/api/api-key";
import {
  createApiKey,
  deleteApiKey,
  fetchTenantApiKeys,
  rotateApiKey,
} from "@/api/tenant";
import {
  Button,
  ConfirmDialog,
  EntityDetailCard,
  FormField,
  FormModal,
  Input,
  Modal,
  NumberInput,
  StatusBadge,
  Textarea,
} from "@/components/ui";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import type {
  ApiKey,
  CreateApiKeyRequest,
  UpdateApiKeyRequest,
} from "@/types/api/api-key";
import { formatDateTime } from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { isBizRole } from "@/utils/ops/roles";
import { logger } from "@/utils/sys/logger";
import { copyWithFeedback } from "@/utils/ui/copy-feedback";
import {
  showErrorToast,
  showSuccessToast,
  showWarningToast,
} from "@/utils/ui/message-guard";
import { TransportEncryptionSection } from "./security/TransportEncryptionSection";
import { WhitelistSection } from "./security/WhitelistSection";

export interface CredentialsAndSecurityTabProps {
  tenantCode: string;
}

export function CredentialsAndSecurityTab({
  tenantCode,
}: CredentialsAndSecurityTabProps) {
  const { t } = useT();
  const role = useAuthStore((s) => s.info.role);
  const canWrite = isBizRole(role);

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

  return (
    <div
      className="space-y-4 min-w-0"
      data-testid="tenant-credentials-security-tab"
    >
      {canWrite && (
        <EntityDetailCard
          title={t("tenant.detail.tabs.apiKeys")}
          icon={<KeyRound className="h-4 w-4" />}
          testId="tenant-api-keys-card"
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
            <div className="py-4 text-center text-xs text-muted-foreground">
              {t("layout.loading")}
            </div>
          ) : apiKeys.length === 0 ? (
            <div className="text-xs text-muted-foreground">
              {t("common.feedback.noData")}
            </div>
          ) : (
            <div className="divide-y divide-border-subtle">
              {apiKeys.map((row) => (
                <div
                  key={row.key_prefix}
                  className="flex flex-wrap items-center justify-between gap-3 py-2 first:pt-0 last:pb-0"
                >
                  <div className="flex flex-wrap items-center gap-3 min-w-0">
                    <code className="rounded bg-muted px-2.5 py-1 font-mono-tech text-xs text-foreground">
                      ****{row.key_suffix}
                    </code>
                    <StatusBadge
                      status={row.is_active ? "ENABLED" : "DISABLED"}
                      label={
                        row.is_active
                          ? t("common.status.enabled")
                          : t("common.status.disabled")
                      }
                    />
                    <span className="text-xs text-muted-foreground">
                      {t("tenant.apiKey.expiry")}:{" "}
                      <span className="font-mono-tech text-foreground">
                        {row.expires_at
                          ? formatDateTime(row.expires_at)
                          : t("common.status.neverExpires")}
                      </span>
                    </span>
                  </div>

                  <div className="inline-flex flex-wrap items-center gap-1.5">
                    {apiKeyActions.rotate !== false && (
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        loading={rotatingPrefix === row.key_prefix}
                        onClick={() => setRotateTarget(row)}
                      >
                        <RefreshCw className="h-3.5 w-3.5" />
                        {t("tenant.actions.rotateApiKey")}
                      </Button>
                    )}
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
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
                        variant="outline"
                        size="sm"
                        className="text-danger-fg hover:text-danger-fg"
                        onClick={() => setRevokeTarget(row)}
                      >
                        {t("tenant.actions.revokeApiKey")}
                      </Button>
                    )}
                  </div>
                </div>
              ))}
            </div>
          )}
        </EntityDetailCard>
      )}

      {canWrite && <TransportEncryptionSection tenantCode={tenantCode} />}

      <WhitelistSection tenantCode={tenantCode} />

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
