import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router";
import {
  AlertTriangle,
  Copy,
  KeyRound,
  Lock,
  LockOpen,
  LogIn,
  Plus,
  RefreshCw,
  Save,
  Trash2,
} from "lucide-react";
import {
  createTenantCryptoKey,
  deleteTenantCryptoKey,
  fetchTenantCrypto,
  rotateTenantCryptoKey,
  updateTenantCryptoProfile,
} from "@/api/tenant";
import {
  CryptoSuiteSelect,
  type CryptoSuiteSelectValue,
} from "@/components/crypto/CryptoSuiteSelect";
import { DEFAULT_CRYPTO_SUITE_ID } from "@/components/crypto/cryptoSuiteState";
import {
  Button,
  ConfirmDialog,
  EntityDetailCard,
  Modal,
  NumberInput,
  StatusBadge,
  Switch,
  Textarea,
  cn,
} from "@/components/ui";
import { useT } from "@/locales";
import type {
  TenantCryptoActions,
  TenantCryptoKey,
  TenantCryptoKeyCreateResponse,
  TenantCryptoLicenseStatus,
  TenantCryptoProfile,
  TenantEncryptionMode,
} from "@/types/api/tenant";
import { getStatusLabel } from "@/utils/display-labels";
import { formatDateTime } from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { logger } from "@/utils/sys/logger";
import { copyWithFeedback } from "@/utils/ui/copy-feedback";
import { showSuccessToast } from "@/utils/ui/message-guard";

export interface TransportEncryptionSectionProps {
  tenantCode: string;
}

const DEFAULT_ACTIONS: TenantCryptoActions = {
  update_profile: false,
  create_key: false,
  rotate_key: false,
  delete_key: false,
  allow_insecure: false,
  manage_root_package: false,
};

export function TransportEncryptionSection({
  tenantCode,
}: TransportEncryptionSectionProps) {
  const { t } = useT();
  const navigate = useNavigate();

  const [isInitialized, setIsInitialized] = useState(false);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [creating, setCreating] = useState(false);
  const [rotating, setRotating] = useState(false);
  const [deleting, setDeleting] = useState(false);

  const [cryptoKey, setCryptoKey] = useState<TenantCryptoKey | null>(null);
  const [licenseStatus, setLicenseStatus] =
    useState<TenantCryptoLicenseStatus | null>(null);
  const [cryptoActions, setCryptoActions] =
    useState<TenantCryptoActions>(DEFAULT_ACTIONS);

  const [encryptionMode, setEncryptionMode] =
    useState<TenantEncryptionMode>("none");
  const [requireSignature, setRequireSignature] = useState(false);
  const [nonceTtlSeconds, setNonceTtlSeconds] = useState(300);
  const [clockSkewSeconds, setClockSkewSeconds] = useState(300);

  const [suiteForm, setSuiteForm] = useState<CryptoSuiteSelectValue>({
    suite: DEFAULT_CRYPTO_SUITE_ID,
    dataAlg: null,
    macAlg: null,
    allowInsecure: false,
  });

  const [revealedKey, setRevealedKey] =
    useState<TenantCryptoKeyCreateResponse | null>(null);
  const [rotateConfirmOpen, setRotateConfirmOpen] = useState(false);
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);

  const abortRef = useRef<AbortController | null>(null);

  const isSuperAdmin = cryptoActions.allow_insecure;
  const canWrite = cryptoActions.update_profile;
  const tenantCryptoEnabled = licenseStatus?.issuanceAllowed === true;

  const applyProfile = useCallback((profile: TenantCryptoProfile) => {
    const mode = profile.encryption_mode;
    setEncryptionMode(mode);
    setRequireSignature(mode === "none" ? false : profile.require_signature);
    setNonceTtlSeconds(profile.nonce_ttl_seconds);
    setClockSkewSeconds(profile.clock_skew_seconds);
    const nextSuite: CryptoSuiteSelectValue = {
      suite: profile.suite ?? null,
      dataAlg: profile.data_alg ?? null,
      macAlg: profile.mac_alg ?? null,
      allowInsecure: profile.allow_insecure === true,
    };
    if (mode !== "none" && !nextSuite.suite && !nextSuite.dataAlg) {
      nextSuite.suite = DEFAULT_CRYPTO_SUITE_ID;
    }
    setSuiteForm(nextSuite);
  }, []);

  const loadData = useCallback(async () => {
    if (!tenantCode) return;
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;
    setLoading(true);
    try {
      const bundle = await fetchTenantCrypto(tenantCode, controller.signal);
      if (controller.signal.aborted) return;
      applyProfile(bundle.profile);
      setCryptoKey(bundle.key ?? null);
      setLicenseStatus(bundle.license_status);
      setCryptoActions(bundle.actions);
      setIsInitialized(true);
    } catch (error) {
      if (controller.signal.aborted) return;
      setLicenseStatus(null);
      setCryptoActions(DEFAULT_ACTIONS);
      setIsInitialized(false);
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to load tenant crypto settings:", error);
      }
    } finally {
      if (!controller.signal.aborted) {
        setLoading(false);
      }
    }
  }, [tenantCode, applyProfile]);

  useEffect(() => {
    void loadData();
    return () => abortRef.current?.abort();
  }, [loadData]);

  const handleSelectMode = (mode: TenantEncryptionMode) => {
    if (!tenantCryptoEnabled || !canWrite) return;
    setEncryptionMode(mode);
    if (mode === "none") {
      setRequireSignature(false);
      setSuiteForm({
        suite: null,
        dataAlg: null,
        macAlg: null,
        allowInsecure: false,
      });
    } else if (!suiteForm.suite && !suiteForm.dataAlg) {
      setSuiteForm((prev) => ({
        ...prev,
        suite: DEFAULT_CRYPTO_SUITE_ID,
      }));
    }
  };

  const modeCards = useMemo(
    () => [
      {
        value: "none" as const,
        label: t("tenant.transportEncryption.modes.none"),
        desc: t("tenant.transportEncryption.modesDesc.none"),
        icon: <LockOpen className="h-4 w-4" />,
      },
      {
        value: "request" as const,
        label: t("tenant.transportEncryption.modes.request"),
        desc: t("tenant.transportEncryption.modesDesc.request"),
        icon: <LogIn className="h-4 w-4" />,
      },
      {
        value: "full" as const,
        label: t("tenant.transportEncryption.modes.full"),
        desc: t("tenant.transportEncryption.modesDesc.full"),
        icon: <Lock className="h-4 w-4" />,
      },
    ],
    [t],
  );

  const missingKeyForEncryption =
    encryptionMode !== "none" && cryptoKey === null;

  const handleSaveProfile = async () => {
    if (
      !tenantCryptoEnabled ||
      !cryptoActions.update_profile ||
      saving ||
      missingKeyForEncryption
    ) {
      return;
    }
    if (suiteForm.allowInsecure && !cryptoActions.allow_insecure) return;
    setSaving(true);
    try {
      await updateTenantCryptoProfile(tenantCode, {
        encryptionMode,
        suite: encryptionMode === "none" ? null : suiteForm.suite,
        dataAlg: encryptionMode === "none" ? null : suiteForm.dataAlg,
        macAlg: encryptionMode === "none" ? null : suiteForm.macAlg,
        allowInsecure:
          encryptionMode === "none" ? false : suiteForm.allowInsecure,
        requireSignature: encryptionMode === "none" ? false : requireSignature,
        nonceTtlSeconds,
        clockSkewSeconds,
      });
      await loadData();
      showSuccessToast(t("common.feedback.saveSuccess"));
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to update transport crypto profile:", error);
      }
    } finally {
      setSaving(false);
    }
  };

  const handleCreateKey = async () => {
    if (!tenantCryptoEnabled || !cryptoActions.create_key || creating) return;
    setCreating(true);
    try {
      const res = await createTenantCryptoKey(tenantCode);
      setRevealedKey(res);
      await loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to create crypto key:", error);
      }
    } finally {
      setCreating(false);
    }
  };

  const handleConfirmRotateKey = async () => {
    if (!tenantCryptoEnabled || !cryptoActions.rotate_key || rotating) return;
    setRotating(true);
    try {
      const res = await rotateTenantCryptoKey(tenantCode);
      setRotateConfirmOpen(false);
      setRevealedKey(res);
      await loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to rotate crypto key:", error);
      }
    } finally {
      setRotating(false);
    }
  };

  const handleConfirmDeleteKey = async () => {
    if (!tenantCryptoEnabled || !cryptoActions.delete_key || deleting) return;
    setDeleting(true);
    try {
      await deleteTenantCryptoKey(tenantCode);
      showSuccessToast(t("common.feedback.deleteSuccess"));
      setDeleteConfirmOpen(false);
      await loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to delete crypto key:", error);
      }
    } finally {
      setDeleting(false);
    }
  };

  const digestPreview = cryptoKey?.secret_key_digest
    ? `${cryptoKey.secret_key_digest.slice(0, 16)}…`
    : "";

  return (
    <div
      className="space-y-3 min-w-0"
      data-testid="tenant-transport-encryption-section"
    >
      {isInitialized && licenseStatus !== null && !tenantCryptoEnabled && (
        <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-warning-border bg-warning-bg px-4 py-3 text-xs text-warning-fg">
          <div className="flex items-start gap-2">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
            <div className="space-y-0.5">
              <div className="font-semibold">
                {t("tenant.transportEncryption.alert.title")}
              </div>
              <div>
                {t("tenant.transportEncryption.alert.currentStatus", {
                  status: getStatusLabel(licenseStatus.status ?? ""),
                })}
              </div>
            </div>
          </div>
          {cryptoActions.manage_root_package && (
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => navigate("/control-panel/system?tab=license")}
            >
              {t("tenant.transportEncryption.actions.goRootPackage")}
            </Button>
          )}
        </div>
      )}

      <EntityDetailCard
        title={t("tenant.transportEncryption.sections.strategy")}
        icon={<Lock className="h-4 w-4" />}
        testId="tenant-transport-crypto-card"
        actions={
          cryptoActions.update_profile ? (
            <Button
              type="button"
              variant="primary"
              size="sm"
              loading={saving}
              disabled={!tenantCryptoEnabled || missingKeyForEncryption}
              onClick={() => void handleSaveProfile()}
              data-testid="tenant-crypto-save-btn"
            >
              <Save className="h-3.5 w-3.5" />
              {t("common.actions.save")}
            </Button>
          ) : undefined
        }
      >
        <div className="space-y-4">
          {/* Unified Single-Key Status Strip */}
          <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border bg-muted/25 px-3.5 py-2.5">
            <div className="flex flex-wrap items-center gap-2.5 min-w-0">
              <span className="inline-flex h-7 w-7 items-center justify-center rounded-md bg-xblue-soft text-xblue shrink-0">
                <KeyRound className="h-3.5 w-3.5" />
              </span>
              <div className="min-w-0 space-y-0.5">
                <div className="flex flex-wrap items-center gap-2 text-xs">
                  <span className="font-semibold text-foreground">
                    {t("tenant.transportEncryption.sections.secretKey")}
                  </span>
                  {loading && !isInitialized ? (
                    <span className="text-muted-foreground">
                      {t("layout.loading")}
                    </span>
                  ) : cryptoKey ? (
                    <StatusBadge
                      status="ACTIVE"
                      label={t("tenant.transportEncryption.keyStatus.ready")}
                    />
                  ) : (
                    <StatusBadge
                      status="DISABLED"
                      label={t(
                        "tenant.transportEncryption.keyStatus.unconfigured",
                      )}
                    />
                  )}
                </div>
                {cryptoKey && (
                  <div className="flex flex-wrap items-center gap-x-3 gap-y-0.5 text-[11px] text-muted-foreground font-mono-tech">
                    <span>
                      {t("tenant.transportEncryption.fields.digest")}:{" "}
                      {digestPreview}
                    </span>
                    <span>
                      {t("common.fields.updatedAt")}:{" "}
                      {formatDateTime(cryptoKey.updated_at)}
                    </span>
                  </div>
                )}
              </div>
            </div>

            <div className="flex flex-wrap items-center gap-2 shrink-0">
              {!cryptoKey && cryptoActions.create_key && (
                <Button
                  type="button"
                  variant="primary"
                  size="sm"
                  loading={creating}
                  disabled={!tenantCryptoEnabled}
                  onClick={() => void handleCreateKey()}
                  data-testid="tenant-crypto-create-key-btn"
                >
                  <Plus className="h-3.5 w-3.5" />
                  {t("tenant.transportEncryption.actions.createKey")}
                </Button>
              )}
              {cryptoKey && cryptoActions.rotate_key && (
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  loading={rotating}
                  disabled={!tenantCryptoEnabled}
                  onClick={() => setRotateConfirmOpen(true)}
                  data-testid="tenant-crypto-rotate-key-btn"
                >
                  <RefreshCw className="h-3.5 w-3.5" />
                  {t("tenant.transportEncryption.actions.rotateKey")}
                </Button>
              )}
              {cryptoKey && cryptoActions.delete_key && (
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={!tenantCryptoEnabled}
                  onClick={() => setDeleteConfirmOpen(true)}
                  className="text-danger-fg hover:text-danger-fg"
                  data-testid="tenant-crypto-delete-key-btn"
                >
                  <Trash2 className="h-3.5 w-3.5" />
                  {t("tenant.transportEncryption.actions.delete")}
                </Button>
              )}
            </div>
          </div>

          {missingKeyForEncryption && (
            <div className="flex items-center gap-2 rounded-md border border-warning-border bg-warning-bg px-3 py-2 text-xs text-warning-fg">
              <AlertTriangle className="h-3.5 w-3.5 shrink-0" />
              <span>
                {t("tenant.transportEncryption.hints.mustCreateKeyBeforeEnable")}
              </span>
            </div>
          )}

          {/* Mode Selector */}
          <div className="space-y-2">
            <div className="text-xs font-medium text-foreground">
              {t("tenant.transportEncryption.fields.encryptionMode")}
            </div>
            <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-3">
              {modeCards.map((opt) => {
                const active = encryptionMode === opt.value;
                const disabled = !tenantCryptoEnabled || !canWrite;
                return (
                  <button
                    key={opt.value}
                    type="button"
                    disabled={disabled}
                    onClick={() => handleSelectMode(opt.value)}
                    className={cn(
                      "flex items-center gap-2.5 rounded-lg border px-3 py-2.5 text-left transition-all cursor-pointer disabled:cursor-not-allowed disabled:opacity-55",
                      active
                        ? "border-primary bg-primary-soft/50 text-foreground"
                        : "border-border bg-card text-foreground hover:border-border-hover hover:bg-hover-bg/60",
                    )}
                  >
                    <span
                      className={cn(
                        "shrink-0",
                        active ? "text-primary" : "text-muted-foreground",
                      )}
                    >
                      {opt.icon}
                    </span>
                    <span className="text-xs font-semibold truncate">
                      {opt.label}
                    </span>
                  </button>
                );
              })}
            </div>
          </div>

          {/* Suite & Replay Protection Controls */}
          <div className="grid grid-cols-1 gap-4 border-t border-border-subtle pt-3.5 xl:grid-cols-2 items-start">
            <div className="space-y-2">
              <div className="text-xs font-medium text-foreground">
                {t("tenant.transportEncryption.fields.suite")}
              </div>
              <CryptoSuiteSelect
                value={suiteForm}
                onChange={setSuiteForm}
                disabled={
                  !tenantCryptoEnabled || !canWrite || encryptionMode === "none"
                }
                canToggleInsecure={isSuperAdmin}
              />
            </div>

            <div className="space-y-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="text-xs font-medium text-foreground">
                  {t("tenant.transportEncryption.fields.nonceTtlSeconds")}
                </span>
                <NumberInput
                  value={nonceTtlSeconds}
                  min={60}
                  max={3600}
                  step={60}
                  disabled={!tenantCryptoEnabled || !canWrite}
                  onChange={setNonceTtlSeconds}
                  className="w-48"
                  aria-label={t(
                    "tenant.transportEncryption.fields.nonceTtlSeconds",
                  )}
                />
              </div>

              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="text-xs font-medium text-foreground">
                  {t("tenant.transportEncryption.fields.clockSkewSeconds")}
                </span>
                <NumberInput
                  value={clockSkewSeconds}
                  min={60}
                  max={3600}
                  step={60}
                  disabled={!tenantCryptoEnabled || !canWrite}
                  onChange={setClockSkewSeconds}
                  className="w-48"
                  aria-label={t(
                    "tenant.transportEncryption.fields.clockSkewSeconds",
                  )}
                />
              </div>

              <div className="flex items-center justify-between gap-4 rounded-md border border-border-subtle px-3 py-2">
                <span className="text-xs font-medium text-foreground">
                  {t("tenant.transportEncryption.fields.signatureCheck")}
                </span>
                <Switch
                  checked={requireSignature}
                  disabled={
                    !tenantCryptoEnabled ||
                    !canWrite ||
                    encryptionMode === "none"
                  }
                  onCheckedChange={setRequireSignature}
                  aria-label={t(
                    "tenant.transportEncryption.fields.signatureCheck",
                  )}
                />
              </div>
            </div>
          </div>
        </div>
      </EntityDetailCard>

      {/* Rotate Key Confirm */}
      <ConfirmDialog
        open={rotateConfirmOpen}
        onClose={() => setRotateConfirmOpen(false)}
        onConfirm={() => void handleConfirmRotateKey()}
        loading={rotating}
        danger={false}
        title={t("tenant.transportEncryption.actions.rotateKey")}
        confirmLabel={t("tenant.transportEncryption.actions.rotateKey")}
        description={t("tenant.transportEncryption.messages.rotateKeyConfirm")}
      />

      {/* Delete Key Confirm */}
      <ConfirmDialog
        open={deleteConfirmOpen}
        onClose={() => setDeleteConfirmOpen(false)}
        onConfirm={() => void handleConfirmDeleteKey()}
        loading={deleting}
        danger
        title={t("tenant.transportEncryption.actions.delete")}
        confirmLabel={t("common.confirm")}
        description={t("tenant.transportEncryption.messages.deleteKeyConfirm")}
      />

      {/* One-time Secret Key Modal */}
      <Modal
        open={Boolean(revealedKey)}
        onClose={() => setRevealedKey(null)}
        title={t("tenant.transportEncryption.secretDialog.title")}
        size="md"
        closeOnBackdrop={false}
        footer={
          <>
            <Button
              type="button"
              variant="outline"
              onClick={() =>
                revealedKey &&
                void copyWithFeedback(revealedKey.tenantSecretKey)
              }
            >
              <Copy className="h-3.5 w-3.5" />
              {t("common.actions.copy")}
            </Button>
            <Button
              type="button"
              variant="primary"
              onClick={() => setRevealedKey(null)}
            >
              {t("common.actions.close")}
            </Button>
          </>
        }
      >
        <div className="flex items-start gap-2 rounded-md border border-warning-border bg-warning-bg px-3 py-2 text-xs text-warning-fg">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
          <span>{t("tenant.transportEncryption.secretDialog.warning")}</span>
        </div>
        {revealedKey && (
          <Textarea
            rows={4}
            readOnly
            value={revealedKey.tenantSecretKey}
            className="font-mono-tech"
          />
        )}
      </Modal>
    </div>
  );
}
