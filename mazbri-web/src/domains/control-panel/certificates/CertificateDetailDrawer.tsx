import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  ChevronDown,
  Copy,
  Download,
  KeyRound,
  Loader2,
  Lock,
  RefreshCw,
  ShieldAlert,
} from "lucide-react";
import {
  fetchCaCertificate,
  fetchCertificateDetail,
  fetchCertificateFullWithPrivateKey,
  getAutoRenewalConfig,
  renewCertificate,
  revokeCertificate,
  updateAutoRenewal,
} from "@/api/certificate";
import {
  AuthorizationTooltipButton,
  Badge,
  Button,
  ConfirmDialog,
  CopyableText,
  Drawer,
  FormField,
  FormModal,
  NumberInput,
  Popover,
  Select,
  StatusBadge,
  Switch,
  Tabs,
  cn,
} from "@/components/ui";
import { useT } from "@/locales";
import type {
  AutoRenewalConfig,
  Certificate,
  PublicCertificateDetailResponse,
} from "@/types/api/certificate";
import { getStatusLabel } from "@/utils/display-labels";
import { EMPTY_PLACEHOLDER, formatDateTime } from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { logger } from "@/utils/sys/logger";
import { copyWithFeedback } from "@/utils/ui/copy-feedback";
import { showErrorToast, showSuccessToast } from "@/utils/ui/message-guard";

const VALIDITY_DAY_OPTIONS = [90, 180, 365, 730, 1095] as const;

type PemTab = "cert" | "ca" | "key";

function mapCertStatusCode(status?: string): string {
  if (status === "valid") return "ACTIVE";
  if (status === "expiring_soon") return "EXPIRING";
  if (status === "expired") return "EXPIRED";
  if (status === "revoked") return "REVOKED";
  return status ?? "";
}

function triggerPemDownload(filename: string, content: string): void {
  const blob = new Blob([content], { type: "application/x-pem-file" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}

export interface CertificateDetailDrawerProps {
  open: boolean;
  certId: string | null;
  initialCert: Certificate | null;
  authorizationDisabled: boolean;
  authorizationDisabledReason: string;
  onClose: () => void;
  onChanged: () => void;
}

export function CertificateDetailDrawer({
  open,
  certId,
  initialCert,
  authorizationDisabled,
  authorizationDisabledReason,
  onClose,
  onChanged,
}: CertificateDetailDrawerProps) {
  const { t } = useT();
  const [cert, setCert] = useState<PublicCertificateDetailResponse | null>(
    null,
  );
  const [loading, setLoading] = useState(false);

  const [activePemTab, setActivePemTab] = useState<PemTab>("cert");
  const [caPem, setCaPem] = useState("");
  const [loadingCa, setLoadingCa] = useState(false);
  const [keyPem, setKeyPem] = useState("");
  const [unlockingKey, setUnlockingKey] = useState(false);

  const [autoRenew, setAutoRenew] = useState<AutoRenewalConfig>({
    enabled: false,
    threshold_days: 30,
    validity_days: 365,
  });
  const [nextRenewalAt, setNextRenewalAt] = useState<string | null>(null);
  const [savingAutoRenew, setSavingAutoRenew] = useState(false);

  const [downloadMenuOpen, setDownloadMenuOpen] = useState(false);
  const [downloadingBundle, setDownloadingBundle] = useState(false);

  const [renewModalOpen, setRenewModalOpen] = useState(false);
  const [renewDays, setRenewDays] = useState(365);
  const [renewing, setRenewing] = useState(false);

  const [revokeConfirmOpen, setRevokeConfirmOpen] = useState(false);
  const [revoking, setRevoking] = useState(false);

  const activeRequestIdRef = useRef(0);

  const loadPublicData = useCallback(
    async (id: string) => {
      const reqId = ++activeRequestIdRef.current;
      setKeyPem("");
      if (authorizationDisabled) {
        setCert(null);
        return;
      }
      setLoading(true);
      try {
        const [detail, autoCfg] = await Promise.all([
          fetchCertificateDetail(id),
          getAutoRenewalConfig(id).catch(() => null),
        ]);
        if (activeRequestIdRef.current !== reqId) return;
        setCert(detail);
        if (autoCfg) {
          setAutoRenew({
            enabled: autoCfg.enabled,
            threshold_days: autoCfg.threshold_days,
            validity_days: autoCfg.validity_days,
          });
          setNextRenewalAt(autoCfg.next_renewal_at);
        } else {
          setAutoRenew({
            enabled: false,
            threshold_days: 30,
            validity_days: 365,
          });
          setNextRenewalAt(null);
        }
      } catch (error) {
        if (activeRequestIdRef.current === reqId) {
          logger.error("Failed to load public certificate detail:", error);
        }
      } finally {
        if (activeRequestIdRef.current === reqId) {
          setLoading(false);
        }
      }
    },
    [authorizationDisabled],
  );

  useEffect(() => {
    if (!open || !certId) {
      activeRequestIdRef.current += 1;
      setCert(null);
      setCaPem("");
      setKeyPem("");
      setActivePemTab("cert");
      setNextRenewalAt(null);
      setDownloadMenuOpen(false);
      setRenewModalOpen(false);
      setRevokeConfirmOpen(false);
      return;
    }
    setActivePemTab("cert");
    void loadPublicData(certId);
  }, [open, certId, loadPublicData]);

  useEffect(() => {
    if (activePemTab !== "ca" || caPem || authorizationDisabled || !open) {
      return;
    }
    let active = true;
    setLoadingCa(true);
    fetchCaCertificate()
      .then((res) => {
        if (active) setCaPem(res.ca_cert_pem || "");
      })
      .catch((error) => {
        logger.error("Failed to load CA certificate:", error);
      })
      .finally(() => {
        if (active) setLoadingCa(false);
      });
    return () => {
      active = false;
    };
  }, [activePemTab, caPem, authorizationDisabled, open]);

  const handleUnlockPrivateKey = async () => {
    if (!cert || authorizationDisabled || unlockingKey) return;
    setUnlockingKey(true);
    try {
      const full = await fetchCertificateFullWithPrivateKey(cert.id);
      setKeyPem(full.key_pem);
      if (full.ca_cert_pem && !caPem) {
        setCaPem(full.ca_cert_pem);
      }
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to unlock certificate private key:", error);
      }
    } finally {
      setUnlockingKey(false);
    }
  };

  const handleDownloadPublicCert = () => {
    if (!cert || authorizationDisabled) return;
    setDownloadMenuOpen(false);
    const safeSubject = cert.subject || "certificate";
    triggerPemDownload(`${safeSubject}-cert.pem`, cert.cert_pem);
  };

  const handleDownloadFullBundle = async () => {
    if (!cert || authorizationDisabled || downloadingBundle) return;
    setDownloadMenuOpen(false);
    setDownloadingBundle(true);
    try {
      const full = await fetchCertificateFullWithPrivateKey(cert.id);
      const safeSubject = cert.subject || "certificate";
      const parts = [
        `=== cert.pem ===\n${full.cert_pem}`,
        `=== key.pem ===\n${full.key_pem}`,
      ];
      if (full.ca_cert_pem) {
        parts.push(`=== ca.pem ===\n${full.ca_cert_pem}`);
      }
      triggerPemDownload(`${safeSubject}-certificates.pem`, parts.join("\n\n"));
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to download full certificate bundle:", error);
      }
    } finally {
      setDownloadingBundle(false);
    }
  };

  const handleSaveAutoRenewal = async () => {
    if (!cert || authorizationDisabled || savingAutoRenew) return;
    if (
      autoRenew.enabled &&
      (autoRenew.threshold_days < 1 || autoRenew.threshold_days > 365)
    ) {
      showErrorToast(
        t("certificate.autoRenewalDialog.validation.thresholdDaysRange"),
      );
      return;
    }
    setSavingAutoRenew(true);
    try {
      await updateAutoRenewal(cert.id, {
        enabled: autoRenew.enabled,
        threshold_days: autoRenew.threshold_days,
        validity_days: autoRenew.validity_days,
      });
      const refreshed = await getAutoRenewalConfig(cert.id).catch(() => null);
      if (refreshed) {
        setNextRenewalAt(refreshed.next_renewal_at);
      }
      showSuccessToast(
        autoRenew.enabled
          ? t("common.feedback.autoRenewEnabled")
          : t("common.feedback.autoRenewDisabled"),
      );
      onChanged();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to update auto-renewal policy:", error);
      }
    } finally {
      setSavingAutoRenew(false);
    }
  };

  const handleConfirmRenew = async () => {
    if (!cert || authorizationDisabled || renewing) return;
    if (!Number.isInteger(renewDays) || renewDays < 1 || renewDays > 36135) {
      showErrorToast(t("certificate.messages.renewDaysRange"));
      return;
    }
    setRenewing(true);
    try {
      await renewCertificate(cert.id, { validity_days: renewDays });
      setKeyPem("");
      setRenewModalOpen(false);
      await loadPublicData(cert.id);
      onChanged();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to renew certificate:", error);
      }
    } finally {
      setRenewing(false);
    }
  };

  const handleConfirmRevoke = async () => {
    if (!cert || authorizationDisabled || revoking) return;
    setRevoking(true);
    try {
      await revokeCertificate(cert.id);
      setKeyPem("");
      setRevokeConfirmOpen(false);
      await loadPublicData(cert.id);
      onChanged();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to revoke certificate:", error);
      }
    } finally {
      setRevoking(false);
    }
  };

  const renewDisabledReason = useMemo(() => {
    if (authorizationDisabled) return authorizationDisabledReason;
    if (!cert) return t("certificate.detail.disabledReasons.notLoaded");
    if (cert.status === "revoked") {
      return t("certificate.detail.disabledReasons.revoked");
    }
    return "";
  }, [authorizationDisabled, authorizationDisabledReason, cert, t]);

  const validitySelectOptions = useMemo(
    () =>
      VALIDITY_DAY_OPTIONS.map((days) => {
        const label =
          days === 90
            ? t("certificate.autoRenewalDialog.options.validityDays.90")
            : days === 180
              ? t("certificate.autoRenewalDialog.options.validityDays.180")
              : days === 365
                ? t("certificate.autoRenewalDialog.options.validityDays.365")
                : days === 730
                  ? t("certificate.autoRenewalDialog.options.validityDays.730")
                  : t(
                      "certificate.autoRenewalDialog.options.validityDays.1095",
                    );
        return { value: String(days), label, text: label };
      }),
    [t],
  );

  const isRevoked = cert?.status === "revoked";
  const headerTitle =
    cert?.subject || initialCert?.subject || t("certificate.detail.title");
  const headerSubtitle = cert?.id || certId || undefined;

  return (
    <>
      <Drawer
        open={open}
        onClose={onClose}
        size="lg"
        testId="certificate-detail-drawer"
        title={
          <span className="inline-flex items-center gap-2 min-w-0">
            <span className="truncate">{headerTitle}</span>
            {cert && (
              <StatusBadge
                status={mapCertStatusCode(cert.status)}
                label={getStatusLabel(cert.status)}
              />
            )}
          </span>
        }
        subtitle={headerSubtitle}
        footer={
          <div className="flex flex-wrap items-center justify-between gap-2 w-full">
            <div>
              {(!cert || !isRevoked) && (
                <AuthorizationTooltipButton
                  type="button"
                  variant="danger"
                  loading={revoking}
                  disabled={authorizationDisabled || !cert}
                  disabledReason={
                    authorizationDisabled
                      ? authorizationDisabledReason
                      : undefined
                  }
                  onClick={() => setRevokeConfirmOpen(true)}
                  data-testid="cert-drawer-revoke-btn"
                >
                  <ShieldAlert className="h-3.5 w-3.5" />
                  {t("certificate.detail.actions.revoke")}
                </AuthorizationTooltipButton>
              )}
            </div>

            <div className="flex items-center gap-2">
              <Popover
                open={downloadMenuOpen}
                onOpenChange={setDownloadMenuOpen}
                align="right"
                contentClassName="w-52 p-1 space-y-0.5 bottom-full top-auto mb-1.5 mt-0"
                trigger={
                  <AuthorizationTooltipButton
                    type="button"
                    variant="outline"
                    loading={downloadingBundle}
                    disabled={authorizationDisabled || !cert}
                    disabledReason={
                      authorizationDisabled
                        ? authorizationDisabledReason
                        : undefined
                    }
                    onClick={() => setDownloadMenuOpen((v) => !v)}
                  >
                    <Download className="h-3.5 w-3.5" />
                    {t("certificate.detail.actions.download")}
                    <ChevronDown className="h-3.5 w-3.5 opacity-70" />
                  </AuthorizationTooltipButton>
                }
              >
                <button
                  type="button"
                  onClick={handleDownloadPublicCert}
                  className="w-full rounded-md px-2.5 py-1.5 text-left text-xs text-foreground hover:bg-hover-bg transition-colors cursor-pointer"
                >
                  {t("certificate.detail.actions.downloadPublicCert")}
                </button>
                <button
                  type="button"
                  onClick={() => void handleDownloadFullBundle()}
                  className="w-full rounded-md px-2.5 py-1.5 text-left text-xs text-foreground hover:bg-hover-bg transition-colors cursor-pointer"
                >
                  {t("certificate.detail.actions.downloadFullBundle")}
                </button>
              </Popover>

              <AuthorizationTooltipButton
                type="button"
                variant="primary"
                loading={renewing}
                disabled={Boolean(renewDisabledReason)}
                disabledReason={renewDisabledReason || undefined}
                onClick={() => {
                  setRenewDays(365);
                  setRenewModalOpen(true);
                }}
              >
                <RefreshCw className="h-3.5 w-3.5" />
                {t("certificate.detail.actions.renew")}
              </AuthorizationTooltipButton>

              <Button type="button" variant="outline" onClick={onClose}>
                {t("common.actions.close")}
              </Button>
            </div>
          </div>
        }
      >
        {authorizationDisabled && (
          <div className="flex items-center gap-2 rounded-lg border border-warning-border bg-warning-bg px-3 py-2.5 text-xs text-warning-fg">
            <AlertTriangle className="h-4 w-4 shrink-0" />
            <span>{authorizationDisabledReason}</span>
          </div>
        )}

        {loading && !cert ? (
          <div className="flex items-center justify-center py-14 text-xs text-muted-foreground gap-2">
            <Loader2 className="h-4 w-4 animate-spin text-primary" />
            <span>{t("layout.loading")}</span>
          </div>
        ) : cert ? (
          <div className="space-y-5">
            {/* 1. Certificate Metadata */}
            <section className="space-y-2.5">
              <h3 className="text-xs font-semibold text-foreground">
                {t("certificate.detail.sections.info")}
              </h3>
              <dl className="grid grid-cols-1 sm:grid-cols-2 border border-border rounded-lg overflow-hidden text-xs">
                <div className="flex flex-col gap-1 p-3 border-b sm:border-r border-border">
                  <dt className="text-[11px] text-muted-foreground">
                    {t("certificate.detail.fields.id")}
                  </dt>
                  <dd className="text-foreground">
                    <CopyableText value={cert.id} truncate />
                  </dd>
                </div>

                <div className="flex flex-col gap-1 p-3 border-b border-border">
                  <dt className="text-[11px] text-muted-foreground">
                    {t("certificate.detail.fields.status")}
                  </dt>
                  <dd>
                    <StatusBadge
                      status={mapCertStatusCode(cert.status)}
                      label={getStatusLabel(cert.status)}
                    />
                  </dd>
                </div>

                <div className="flex flex-col gap-1 p-3 border-b sm:border-r border-border">
                  <dt className="text-[11px] text-muted-foreground">
                    {t("certificate.detail.fields.subject")}
                  </dt>
                  <dd className="text-foreground font-medium">
                    <CopyableText value={cert.subject} />
                  </dd>
                </div>

                <div className="flex flex-col gap-1 p-3 border-b border-border">
                  <dt className="text-[11px] text-muted-foreground">
                    {t("certificate.detail.fields.renewalCount")}
                  </dt>
                  <dd className="font-mono-tech tabular-nums text-foreground">
                    {cert.renewal_count}
                  </dd>
                </div>

                <div className="sm:col-span-2 flex flex-col gap-1.5 p-3 border-b border-border">
                  <dt className="text-[11px] text-muted-foreground">
                    {t("certificate.detail.fields.dnsNames")}
                  </dt>
                  <dd>
                    {cert.dns_names?.length ? (
                      <div className="flex flex-wrap gap-1.5">
                        {cert.dns_names.map((name) => (
                          <Badge key={name} variant="outline">
                            {name}
                          </Badge>
                        ))}
                      </div>
                    ) : (
                      <span className="text-muted-foreground">
                        {EMPTY_PLACEHOLDER}
                      </span>
                    )}
                  </dd>
                </div>

                <div className="sm:col-span-2 flex flex-col gap-1 p-3 border-b border-border">
                  <dt className="text-[11px] text-muted-foreground">
                    {t("certificate.detail.fields.fingerprint")}
                  </dt>
                  <dd className="text-foreground break-all">
                    {cert.fingerprint ? (
                      <CopyableText
                        value={cert.fingerprint}
                        copyTooltip={t(
                          "certificate.detail.actions.copyFingerprint",
                        )}
                      />
                    ) : (
                      <span className="text-muted-foreground">
                        {EMPTY_PLACEHOLDER}
                      </span>
                    )}
                  </dd>
                </div>

                <div className="flex flex-col gap-1 p-3 border-b sm:border-b-0 sm:border-r border-border">
                  <dt className="text-[11px] text-muted-foreground">
                    {t("certificate.detail.fields.issuedAt")}
                  </dt>
                  <dd className="font-mono-tech tabular-nums text-foreground">
                    {formatDateTime(cert.issued_at)}
                  </dd>
                </div>

                <div className="flex flex-col gap-1 p-3">
                  <dt className="text-[11px] text-muted-foreground">
                    {t("certificate.detail.fields.expiresAt")}
                  </dt>
                  <dd
                    className={cn(
                      "font-mono-tech tabular-nums",
                      cert.status === "expired"
                        ? "text-danger-fg font-semibold"
                        : cert.status === "expiring_soon"
                          ? "text-warning-fg font-semibold"
                          : "text-foreground",
                    )}
                  >
                    {formatDateTime(cert.expires_at)}
                  </dd>
                </div>

                {cert.revoked && cert.revoked_at && (
                  <div className="sm:col-span-2 flex flex-col gap-1 p-3 border-t border-border">
                    <dt className="text-[11px] text-muted-foreground">
                      {t("certificate.detail.fields.revokedAt")}
                    </dt>
                    <dd className="font-mono-tech tabular-nums text-danger-fg">
                      {formatDateTime(cert.revoked_at)}
                    </dd>
                  </div>
                )}
              </dl>
            </section>

            {/* 2. Auto-renewal Policy */}
            <section className="rounded-lg border border-border bg-card overflow-hidden">
              <div className="flex items-center justify-between gap-4 border-b border-border px-3.5 py-3">
                <div className="min-w-0 space-y-0.5">
                  <div className="text-xs font-semibold text-foreground">
                    {t("certificate.detail.autoRenewal.title")}
                  </div>
                  <div className="text-[11px] text-muted-foreground">
                    {t("certificate.detail.autoRenewal.subtitle")}
                  </div>
                </div>
                <div className="flex items-center gap-2 shrink-0">
                  <Badge variant={autoRenew.enabled ? "success" : "neutral"}>
                    {autoRenew.enabled
                      ? t("certificate.detail.autoRenewal.enabledTag")
                      : t("certificate.detail.autoRenewal.disabledTag")}
                  </Badge>
                  <Switch
                    checked={autoRenew.enabled}
                    disabled={authorizationDisabled || isRevoked}
                    onCheckedChange={(checked) =>
                      setAutoRenew((prev) => ({ ...prev, enabled: checked }))
                    }
                    aria-label={t("certificate.detail.autoRenewal.title")}
                  />
                </div>
              </div>

              {autoRenew.enabled && (
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3.5 p-3.5 border-b border-border">
                  <FormField
                    label={t("certificate.detail.autoRenewal.thresholdDays")}
                    hint={t("certificate.autoRenewalDialog.hints.thresholdDays")}
                  >
                    <NumberInput
                      value={autoRenew.threshold_days}
                      min={1}
                      max={365}
                      step={1}
                      disabled={authorizationDisabled || isRevoked}
                      onChange={(next) =>
                        setAutoRenew((prev) => ({
                          ...prev,
                          threshold_days: next,
                        }))
                      }
                      aria-label={t(
                        "certificate.detail.autoRenewal.thresholdDays",
                      )}
                    />
                  </FormField>

                  <FormField
                    label={t("certificate.detail.autoRenewal.validityDays")}
                    hint={t("certificate.autoRenewalDialog.hints.validityDays")}
                  >
                    <Select
                      value={String(autoRenew.validity_days)}
                      disabled={authorizationDisabled || isRevoked}
                      onChange={(e) =>
                        setAutoRenew((prev) => ({
                          ...prev,
                          validity_days: Number(e.target.value),
                        }))
                      }
                      options={validitySelectOptions}
                      className="w-full"
                    />
                  </FormField>
                </div>
              )}

              <div className="flex items-center justify-between gap-3 px-3.5 py-2.5 bg-card">
                <div className="text-xs text-muted-foreground">
                  <span>
                    {t("certificate.detail.autoRenewal.nextRenewalAt")}:{" "}
                  </span>
                  <span className="font-mono-tech tabular-nums text-foreground">
                    {autoRenew.enabled && nextRenewalAt
                      ? formatDateTime(nextRenewalAt)
                      : EMPTY_PLACEHOLDER}
                  </span>
                </div>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  loading={savingAutoRenew}
                  disabled={authorizationDisabled || isRevoked}
                  onClick={() => void handleSaveAutoRenewal()}
                >
                  {t("certificate.detail.autoRenewal.saveAction")}
                </Button>
              </div>
            </section>

            {/* 3. Certificate PEM Files */}
            <section className="space-y-2.5">
              <h3 className="text-xs font-semibold text-foreground">
                {t("certificate.detail.sections.files")}
              </h3>
              <Tabs<PemTab>
                value={activePemTab}
                onChange={setActivePemTab}
                items={[
                  { key: "cert", label: t("certificate.detail.tabs.certPem") },
                  { key: "ca", label: t("certificate.detail.tabs.caPem") },
                  { key: "key", label: t("certificate.detail.tabs.keyPem") },
                ]}
              />

              {activePemTab === "cert" && (
                <div className="space-y-2">
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-[11px] text-muted-foreground">
                      {t("certificate.detail.pem.publicCertHint")}
                    </span>
                    <div className="flex items-center gap-1.5">
                      <Button
                        type="button"
                        variant="outline"
                        size="xs"
                        disabled={!cert.cert_pem}
                        onClick={() => void copyWithFeedback(cert.cert_pem)}
                      >
                        <Copy className="h-3 w-3" />
                        {t("common.actions.copy")}
                      </Button>
                      <Button
                        type="button"
                        variant="outline"
                        size="xs"
                        disabled={!cert.cert_pem}
                        onClick={handleDownloadPublicCert}
                      >
                        <Download className="h-3 w-3" />
                        {t("certificate.detail.actions.download")}
                      </Button>
                    </div>
                  </div>
                  <pre className="max-h-60 overflow-auto rounded-md border border-border bg-muted/30 p-3 font-mono-tech text-[11px] leading-relaxed text-foreground whitespace-pre-wrap break-all">
                    {cert.cert_pem}
                  </pre>
                </div>
              )}

              {activePemTab === "ca" && (
                <div className="space-y-2">
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-[11px] text-muted-foreground">
                      {t("certificate.detail.pem.caCertHint")}
                    </span>
                    <Button
                      type="button"
                      variant="outline"
                      size="xs"
                      disabled={!caPem}
                      onClick={() => void copyWithFeedback(caPem)}
                    >
                      <Copy className="h-3 w-3" />
                      {t("common.actions.copy")}
                    </Button>
                  </div>
                  {loadingCa ? (
                    <div className="flex items-center justify-center py-8 text-xs text-muted-foreground gap-2 rounded-md border border-border bg-card">
                      <Loader2 className="h-4 w-4 animate-spin text-primary" />
                      <span>{t("layout.loading")}</span>
                    </div>
                  ) : (
                    <pre className="max-h-60 overflow-auto rounded-md border border-border bg-muted/30 p-3 font-mono-tech text-[11px] leading-relaxed text-foreground whitespace-pre-wrap break-all">
                      {caPem || EMPTY_PLACEHOLDER}
                    </pre>
                  )}
                </div>
              )}

              {activePemTab === "key" &&
                (!keyPem ? (
                  <div className="flex flex-col items-center justify-center gap-2.5 rounded-lg border border-border bg-card py-8 px-4 text-center">
                    <div className="inline-flex h-10 w-10 items-center justify-center rounded-full bg-primary-soft text-primary">
                      <KeyRound className="h-5 w-5" />
                    </div>
                    <div className="text-xs font-semibold text-foreground">
                      {t("certificate.detail.privateKeyGuard.title")}
                    </div>
                    <p className="max-w-md text-[11px] text-muted-foreground leading-relaxed">
                      {t("certificate.detail.privateKeyGuard.description")}
                    </p>
                    <AuthorizationTooltipButton
                      type="button"
                      variant="primary"
                      size="sm"
                      loading={unlockingKey}
                      disabled={authorizationDisabled}
                      disabledReason={
                        authorizationDisabled
                          ? authorizationDisabledReason
                          : undefined
                      }
                      onClick={() => void handleUnlockPrivateKey()}
                    >
                      {t("certificate.detail.privateKeyGuard.unlockAction")}
                    </AuthorizationTooltipButton>
                  </div>
                ) : (
                  <div className="space-y-2">
                    <div className="flex items-center justify-between gap-2">
                      <span className="text-[11px] text-warning-fg font-medium">
                        {t("certificate.detail.privateKeyGuard.unlockedHint")}
                      </span>
                      <div className="flex items-center gap-1.5">
                        <Button
                          type="button"
                          variant="outline"
                          size="xs"
                          onClick={() => setKeyPem("")}
                        >
                          <Lock className="h-3 w-3" />
                          {t("certificate.detail.privateKeyGuard.lockAction")}
                        </Button>
                        <Button
                          type="button"
                          variant="primary"
                          size="xs"
                          onClick={() => void copyWithFeedback(keyPem)}
                        >
                          <Copy className="h-3 w-3" />
                          {t("common.actions.copy")}
                        </Button>
                      </div>
                    </div>
                    <pre className="max-h-60 overflow-auto rounded-md border border-warning-border bg-muted/30 p-3 font-mono-tech text-[11px] leading-relaxed text-foreground whitespace-pre-wrap break-all">
                      {keyPem}
                    </pre>
                  </div>
                ))}
            </section>
          </div>
        ) : null}
      </Drawer>

      <FormModal
        open={renewModalOpen}
        onClose={() => setRenewModalOpen(false)}
        onSubmit={handleConfirmRenew}
        title={t("certificate.messages.renewPromptTitle")}
        size="sm"
        submitLabel={t("certificate.messages.confirmRenew")}
        cancelLabel={t("common.cancel")}
        submitting={renewing}
      >
        <p className="text-xs text-muted-foreground">
          {t("certificate.messages.renewPromptMessage")}
        </p>
        <FormField
          label={t("certificate.issueDialog.validity")}
          hint={t("certificate.messages.renewPromptPlaceholder")}
        >
          <NumberInput
            value={renewDays}
            min={1}
            max={36135}
            step={1}
            onChange={setRenewDays}
            aria-label={t("certificate.issueDialog.validity")}
          />
        </FormField>
      </FormModal>

      <ConfirmDialog
        open={revokeConfirmOpen}
        onClose={() => setRevokeConfirmOpen(false)}
        onConfirm={() => void handleConfirmRevoke()}
        title={t("common.warning")}
        description={t("certificate.messages.revokeConfirm", {
          name: cert?.subject ?? "",
        })}
        confirmLabel={t("certificate.messages.confirmRevoke")}
        cancelLabel={t("common.cancel")}
        danger
        loading={revoking}
      />
    </>
  );
}
