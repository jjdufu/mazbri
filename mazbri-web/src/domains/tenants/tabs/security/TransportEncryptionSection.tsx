import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router";
import {
  AlertTriangle,
  Copy,
  KeyRound,
  Lock,
  LockOpen,
  LogIn,
  RefreshCw,
  Save,
  ShieldCheck,
} from "lucide-react";
import {
  activateTenantCryptoKey,
  createTenantCryptoKey,
  deleteTenantCryptoKey,
  disableTenantCryptoKey,
  fetchTenantCrypto,
  rotateTenantCryptoKey,
  updateTenantCryptoProfile,
} from "@/api/tenant";
import {
  CryptoSuiteSelect,
  type CryptoSuiteSelectValue,
} from "@/components/crypto/CryptoSuiteSelect";
import {
  Button,
  ConfirmDialog,
  EmptyState,
  EntityDetailCard,
  Modal,
  NumberInput,
  Select,
  StatusBadge,
  Switch,
  Textarea,
  cn,
  type SelectOptionItem,
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
import { EMPTY_PLACEHOLDER, formatDateTime } from "@/utils/format";
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
  activate_key: false,
  deprecate_key: false,
  disable_key: false,
  delete_key: false,
  allow_insecure: false,
  manage_root_package: false,
};

function splitDateTime(value: string | undefined): {
  date: string;
  time: string;
} {
  const formatted = formatDateTime(value);
  const idx = formatted.indexOf(" ");
  if (idx === -1) return { date: formatted, time: "" };
  return {
    date: formatted.slice(0, idx),
    time: formatted.slice(idx + 1),
  };
}

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

  const [keys, setKeys] = useState<TenantCryptoKey[]>([]);
  const [licenseStatus, setLicenseStatus] =
    useState<TenantCryptoLicenseStatus | null>(null);
  const [cryptoActions, setCryptoActions] =
    useState<TenantCryptoActions>(DEFAULT_ACTIONS);

  const [encryptionMode, setEncryptionMode] =
    useState<TenantEncryptionMode>("none");
  const [requireSignature, setRequireSignature] = useState(false);
  const [currentKeyVersion, setCurrentKeyVersion] = useState("");
  const [nonceTtlSeconds, setNonceTtlSeconds] = useState(300);
  const [clockSkewSeconds, setClockSkewSeconds] = useState(300);

  const [suiteForm, setSuiteForm] = useState<CryptoSuiteSelectValue>({
    suite: "suite/tenant-default-2026-11",
    dataAlg: null,
    macAlg: null,
    allowInsecure: false,
  });

  const [revealedKey, setRevealedKey] =
    useState<TenantCryptoKeyCreateResponse | null>(null);
  const [deleteVersion, setDeleteVersion] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);

  const abortRef = useRef<AbortController | null>(null);

  const isSuperAdmin = cryptoActions.allow_insecure;
  const canWrite = cryptoActions.update_profile;
  const tenantCryptoEnabled = licenseStatus?.issuanceAllowed === true;

  const applyProfile = useCallback((profile: TenantCryptoProfile) => {
    const mode = profile.encryption_mode;
    setEncryptionMode(mode);
    setRequireSignature(mode === "none" ? false : profile.require_signature);
    setCurrentKeyVersion(profile.current_key_version ?? "");
    setNonceTtlSeconds(profile.nonce_ttl_seconds);
    setClockSkewSeconds(profile.clock_skew_seconds);
    const nextSuite: CryptoSuiteSelectValue = {
      suite: profile.suite ?? null,
      dataAlg: profile.data_alg ?? null,
      macAlg: profile.mac_alg ?? null,
      allowInsecure: profile.allow_insecure === true,
    };
    if (mode !== "none" && !nextSuite.suite && !nextSuite.dataAlg) {
      nextSuite.suite = "suite/tenant-default-2026-11";
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
      setKeys(bundle.keys ?? []);
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
        suite: "suite/tenant-default-2026-11",
      }));
    }
  };

  const activeKeyOptions = useMemo<SelectOptionItem[]>(
    () => [
      {
        value: "",
        label: EMPTY_PLACEHOLDER,
        text: EMPTY_PLACEHOLDER,
      },
      ...keys
        .filter((k) => k.status === "active")
        .map((k) => ({
          value: k.key_version,
          label: k.key_version,
          text: k.key_version,
        })),
    ],
    [keys],
  );

  const effectiveAlgorithmSummary = useMemo(() => {
    if (encryptionMode === "none") return EMPTY_PLACEHOLDER;
    if (suiteForm.suite) return suiteForm.suite;
    const parts = [suiteForm.dataAlg, suiteForm.macAlg].filter(Boolean);
    return parts.length > 0 ? parts.join(" + ") : EMPTY_PLACEHOLDER;
  }, [encryptionMode, suiteForm]);

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

  const handleSaveProfile = async () => {
    if (!tenantCryptoEnabled || !cryptoActions.update_profile || saving) return;
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
        requireSignature,
        nonceTtlSeconds,
        clockSkewSeconds,
        currentKeyVersion: currentKeyVersion || undefined,
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

  const handleRotateKey = async () => {
    if (!tenantCryptoEnabled || !cryptoActions.rotate_key || rotating) return;
    setRotating(true);
    try {
      const res = await rotateTenantCryptoKey(tenantCode);
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

  const handleSetKeyStatus = async (
    version: string,
    action: "activate" | "disable",
  ) => {
    if (!tenantCryptoEnabled) return;
    if (action === "activate" && !cryptoActions.activate_key) return;
    if (action === "disable" && !cryptoActions.disable_key) return;
    try {
      if (action === "activate") {
        await activateTenantCryptoKey(tenantCode, version);
      } else {
        await disableTenantCryptoKey(tenantCode, version);
      }
      await loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to update crypto key status:", error);
      }
    }
  };

  const handleConfirmDeleteKey = async () => {
    if (
      !deleteVersion ||
      !tenantCryptoEnabled ||
      !cryptoActions.delete_key ||
      deleting
    ) {
      return;
    }
    setDeleting(true);
    try {
      await deleteTenantCryptoKey(tenantCode, deleteVersion);
      showSuccessToast(t("common.feedback.deleteSuccess"));
      setDeleteVersion(null);
      await loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to delete crypto key:", error);
      }
    } finally {
      setDeleting(false);
    }
  };

  const modeLabel =
    encryptionMode === "full"
      ? t("tenant.transportEncryption.modes.full")
      : encryptionMode === "request"
        ? t("tenant.transportEncryption.modes.request")
        : t("tenant.transportEncryption.modes.none");

  return (
    <div
      className="space-y-4 min-w-0"
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

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2 items-start">
        {/* Left: Strategy Card */}
        <EntityDetailCard
          title={t("tenant.transportEncryption.sections.strategy")}
          icon={<Lock className="h-4 w-4" />}
          actions={
            cryptoActions.update_profile ? (
              <Button
                type="button"
                variant="primary"
                size="sm"
                loading={saving}
                disabled={!tenantCryptoEnabled}
                onClick={() => void handleSaveProfile()}
              >
                <Save className="h-3.5 w-3.5" />
                {t("common.actions.save")}
              </Button>
            ) : undefined
          }
        >
          <div className="space-y-4">
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
                        "flex items-start gap-2.5 rounded-lg border p-3 text-left transition-all cursor-pointer disabled:cursor-not-allowed disabled:opacity-55",
                        active
                          ? "border-primary bg-primary-soft/50 text-foreground"
                          : "border-border bg-card text-foreground hover:border-border-hover hover:bg-hover-bg/60",
                      )}
                    >
                      <span
                        className={cn(
                          "mt-0.5 shrink-0",
                          active ? "text-primary" : "text-muted-foreground",
                        )}
                      >
                        {opt.icon}
                      </span>
                      <span className="min-w-0 space-y-0.5">
                        <span className="block text-xs font-semibold">
                          {opt.label}
                        </span>
                        <span className="block text-[11px] leading-snug text-muted-foreground">
                          {opt.desc}
                        </span>
                      </span>
                    </button>
                  );
                })}
              </div>
            </div>

            <div className="space-y-2 border-t border-border-subtle pt-3">
              <div className="text-xs font-medium text-foreground">
                {t("tenant.transportEncryption.fields.suite")}
              </div>
              <CryptoSuiteSelect
                value={suiteForm}
                onChange={setSuiteForm}
                disabled={
                  !tenantCryptoEnabled ||
                  !canWrite ||
                  encryptionMode === "none"
                }
                canToggleInsecure={isSuperAdmin}
              />
            </div>

            <div className="space-y-3 border-t border-border-subtle pt-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="text-xs font-medium text-foreground">
                  {t("tenant.transportEncryption.fields.currentKeyVersion")}
                </span>
                <Select
                  value={currentKeyVersion}
                  options={activeKeyOptions}
                  disabled={
                    encryptionMode === "none" ||
                    !tenantCryptoEnabled ||
                    !canWrite
                  }
                  placeholder={t(
                    "tenant.transportEncryption.placeholders.selectActiveKey",
                  )}
                  onChange={(e) => setCurrentKeyVersion(e.target.value)}
                  className="w-52"
                />
              </div>

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
                  className="w-52"
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
                  className="w-52"
                  aria-label={t(
                    "tenant.transportEncryption.fields.clockSkewSeconds",
                  )}
                />
              </div>

              <div className="flex items-center justify-between gap-4 rounded-md border border-border-subtle px-3 py-2.5">
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
        </EntityDetailCard>

        {/* Right: Key Versions + Effective Contract */}
        <div className="space-y-4 min-w-0">
          <EntityDetailCard
            title={t("tenant.transportEncryption.sections.keyVersions")}
            icon={<KeyRound className="h-4 w-4" />}
            bodyClassName="p-0"
            actions={
              <>
                {cryptoActions.create_key && (
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    loading={creating}
                    disabled={!tenantCryptoEnabled}
                    onClick={() => void handleCreateKey()}
                  >
                    {t("tenant.transportEncryption.actions.createKey")}
                  </Button>
                )}
                {cryptoActions.rotate_key && (
                  <Button
                    type="button"
                    variant="primary"
                    size="sm"
                    loading={rotating}
                    disabled={!tenantCryptoEnabled}
                    onClick={() => void handleRotateKey()}
                  >
                    <RefreshCw className="h-3.5 w-3.5" />
                    {t("tenant.transportEncryption.actions.rotateKey")}
                  </Button>
                )}
              </>
            }
          >
            {loading && keys.length === 0 ? (
              <div className="py-8 text-center text-xs text-muted-foreground">
                {t("layout.loading")}
              </div>
            ) : keys.length === 0 ? (
              <div className="p-4">
                <EmptyState title={t("common.feedback.noData")} />
              </div>
            ) : (
              <div className="overflow-x-auto">
                <table className="w-full border-collapse text-xs text-foreground">
                  <thead>
                    <tr className="border-b border-border bg-muted/45 text-muted-foreground">
                      <th className="h-9 px-3 text-left font-medium whitespace-nowrap">
                        {t("tenant.transportEncryption.table.version")}
                      </th>
                      <th className="h-9 px-2.5 text-left font-medium whitespace-nowrap">
                        {t("tenant.transportEncryption.table.status")}
                      </th>
                      <th className="h-9 px-2.5 text-left font-medium whitespace-nowrap">
                        {t("tenant.transportEncryption.table.algorithmSuite")}
                      </th>
                      <th className="h-9 px-2.5 text-left font-medium whitespace-nowrap">
                        {t("tenant.transportEncryption.table.createdAt")}
                      </th>
                      <th className="h-9 px-3 text-right font-medium whitespace-nowrap">
                        {t("tenant.transportEncryption.table.operations")}
                      </th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-border-subtle">
                    {keys.map((row) => {
                      const dt = splitDateTime(row.created_at);
                      return (
                        <tr
                          key={row.key_version}
                          className="transition-colors hover:bg-hover-bg/75"
                        >
                          <td className="px-3 py-2 font-mono-tech font-medium">
                            {row.key_version}
                          </td>
                          <td className="px-2.5 py-2">
                            <StatusBadge
                              status={row.status}
                              label={getStatusLabel(row.status)}
                            />
                          </td>
                          <td className="px-2.5 py-2">
                            <div className="font-mono-tech text-foreground">
                              {row.data_algorithm}
                            </div>
                            {row.signature_algorithm && (
                              <div className="font-mono-tech text-[11px] text-muted-foreground">
                                {row.signature_algorithm}
                              </div>
                            )}
                          </td>
                          <td className="px-2.5 py-2 font-mono-tech">
                            <div>{dt.date}</div>
                            {dt.time && (
                              <div className="text-[11px] text-muted-foreground">
                                {dt.time}
                              </div>
                            )}
                          </td>
                          <td className="px-3 py-2 text-right">
                            <div className="inline-flex items-center justify-end gap-1">
                              {cryptoActions.activate_key &&
                                row.status !== "active" && (
                                  <Button
                                    type="button"
                                    variant="link"
                                    size="xs"
                                    disabled={!tenantCryptoEnabled}
                                    onClick={() =>
                                      void handleSetKeyStatus(
                                        row.key_version,
                                        "activate",
                                      )
                                    }
                                  >
                                    {t(
                                      "tenant.transportEncryption.actions.activate",
                                    )}
                                  </Button>
                                )}
                              {cryptoActions.disable_key &&
                                row.status !== "disabled" && (
                                  <Button
                                    type="button"
                                    variant="link"
                                    size="xs"
                                    className="text-warning-fg hover:text-warning-fg"
                                    disabled={!tenantCryptoEnabled}
                                    onClick={() =>
                                      void handleSetKeyStatus(
                                        row.key_version,
                                        "disable",
                                      )
                                    }
                                  >
                                    {t(
                                      "tenant.transportEncryption.actions.disable",
                                    )}
                                  </Button>
                                )}
                              {cryptoActions.delete_key && (
                                <Button
                                  type="button"
                                  variant="link"
                                  size="xs"
                                  className="text-danger-fg hover:text-danger-fg"
                                  disabled={!tenantCryptoEnabled}
                                  onClick={() =>
                                    setDeleteVersion(row.key_version)
                                  }
                                >
                                  {t(
                                    "tenant.transportEncryption.actions.delete",
                                  )}
                                </Button>
                              )}
                            </div>
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}
          </EntityDetailCard>

          <EntityDetailCard
            title={t("tenant.transportEncryption.sections.effectiveContract")}
            icon={<ShieldCheck className="h-4 w-4" />}
          >
            <div className="divide-y divide-border-subtle rounded-md border border-border">
              <div className="flex items-center justify-between gap-3 px-3 py-2 text-xs">
                <span className="text-muted-foreground">
                  {t("tenant.transportEncryption.fields.encryptionMode")}
                </span>
                <span className="font-medium text-foreground">{modeLabel}</span>
              </div>
              <div className="flex items-center justify-between gap-3 px-3 py-2 text-xs">
                <span className="text-muted-foreground">
                  {t("tenant.transportEncryption.fields.suite")}
                </span>
                <span className="font-mono-tech font-medium text-foreground">
                  {effectiveAlgorithmSummary}
                </span>
              </div>
              <div className="flex items-center justify-between gap-3 px-3 py-2 text-xs">
                <span className="text-muted-foreground">
                  {t("tenant.transportEncryption.fields.currentKeyVersion")}
                </span>
                <span className="font-mono-tech font-medium text-foreground">
                  {currentKeyVersion || EMPTY_PLACEHOLDER}
                </span>
              </div>
              <div className="flex items-center justify-between gap-3 px-3 py-2 text-xs">
                <span className="text-muted-foreground">
                  {t("tenant.transportEncryption.fields.signatureCheck")}
                </span>
                <StatusBadge
                  status={requireSignature ? "ENABLED" : "DISABLED"}
                  label={
                    requireSignature
                      ? t("common.status.enabled")
                      : t("common.status.disabled")
                  }
                />
              </div>
              <div className="flex items-center justify-between gap-3 px-3 py-2 text-xs">
                <span className="text-muted-foreground">
                  {t("tenant.transportEncryption.fields.nonceTtlSeconds")}
                </span>
                <span className="font-mono-tech font-medium text-foreground">
                  {nonceTtlSeconds}s / ±{clockSkewSeconds}s
                </span>
              </div>
            </div>
          </EntityDetailCard>
        </div>
      </div>

      {/* Delete Key Confirm */}
      <ConfirmDialog
        open={Boolean(deleteVersion)}
        onClose={() => setDeleteVersion(null)}
        onConfirm={() => void handleConfirmDeleteKey()}
        loading={deleting}
        danger
        title={t("common.tips")}
        confirmLabel={t("common.confirm")}
        description={
          deleteVersion
            ? t("tenant.transportEncryption.messages.deleteKeyConfirm", {
                version: deleteVersion,
              })
            : ""
        }
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
          <>
            <div className="divide-y divide-border-subtle rounded-md border border-border">
              <div className="flex items-center justify-between gap-3 px-3 py-2 text-xs">
                <span className="text-muted-foreground">
                  {t(
                    "tenant.transportEncryption.secretDialog.fields.keyVersion",
                  )}
                </span>
                <span className="font-mono-tech font-medium">
                  {revealedKey.keyVersion}
                </span>
              </div>
              <div className="flex items-center justify-between gap-3 px-3 py-2 text-xs">
                <span className="text-muted-foreground">
                  {t(
                    "tenant.transportEncryption.secretDialog.fields.algorithm",
                  )}
                </span>
                <span className="font-mono-tech font-medium">
                  {revealedKey.algorithm}
                </span>
              </div>
              <div className="flex items-center justify-between gap-3 px-3 py-2 text-xs">
                <span className="text-muted-foreground">
                  {t(
                    "tenant.transportEncryption.secretDialog.fields.signatureAlgorithm",
                  )}
                </span>
                <span className="font-mono-tech font-medium">
                  {revealedKey.signatureAlgorithm}
                </span>
              </div>
            </div>
            <Textarea
              rows={4}
              readOnly
              value={revealedKey.tenantSecretKey}
              className="font-mono-tech"
            />
          </>
        )}
      </Modal>
    </div>
  );
}
