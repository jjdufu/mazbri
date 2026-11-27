import { type FormEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Copy, KeyRound, ShieldCheck } from "lucide-react";
import { QRCodeSVG } from "qrcode.react";
import {
  Badge,
  Button,
  Card,
  ConfirmDialog,
  Drawer,
  FormField,
  Input,
  Modal,
  PasswordInput,
  cn,
} from "@/components/ui";
import {
  changePassword,
  disableMfa,
  enableMfa,
  fetchGetUserInfo,
  getMfaSetup,
  getMfaStatus,
  regenerateBackupCodes,
} from "@/api/auth";
import { fetchPasswordPolicy } from "@/api/settings";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import { useMfaStepUpStore } from "@/stores/mfa-step-up";
import { usePersonalSecurityStore } from "@/stores/personal-security";
import type {
  ChangePasswordRequest,
  MfaSessionRefresh,
  MfaSetupResponse,
  MfaStatus,
  PasswordPolicy,
} from "@/types/api";
import { HttpError } from "@/utils/http/error";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-dialog-service";
import {
  formatExpiryUtc8,
  readSessionExpiresIn,
} from "@/utils/http/mfa-step-up-state";
import { isAdminRole } from "@/utils/ops/roles";
import {
  formatPasswordPolicySummaryLines,
  resolvePasswordPolicyPayload,
} from "@/utils/password";
import { logger } from "@/utils/sys/logger";
import { copyWithFeedback } from "@/utils/ui/copy-feedback";
import { showWarningToast } from "@/utils/ui/message-guard";

function isWindowUnavailable(error: unknown): boolean {
  if (!(error instanceof HttpError) || error.code !== 503) return false;
  const data = error.data;
  return (
    !!data &&
    typeof data === "object" &&
    (data as { code?: unknown }).code === "MFA_CHECK_UNAVAILABLE"
  );
}

export function PersonalSecurityDrawer() {
  const { t } = useT();
  const open = usePersonalSecurityStore((s) => s.open);
  const focusSection = usePersonalSecurityStore((s) => s.focusSection);
  const closeDrawer = usePersonalSecurityStore((s) => s.closeDrawer);

  const userInfo = useAuthStore((s) => s.info);
  const setUserInfo = useAuthStore((s) => s.setUserInfo);
  const setToken = useAuthStore((s) => s.setToken);
  const isAdmin = isAdminRole(userInfo.role);

  const mfaSectionRef = useRef<HTMLDivElement | null>(null);
  const passwordSectionRef = useRef<HTMLDivElement | null>(null);

  const [statusLoading, setStatusLoading] = useState(false);
  const [actionLoading, setActionLoading] = useState(false);
  const [passwordLoading, setPasswordLoading] = useState(false);

  const [status, setStatus] = useState<MfaStatus | null>(null);
  const [setupData, setSetupData] = useState<MfaSetupResponse | null>(null);
  const [setupCode, setSetupCode] = useState("");
  const [backupCodes, setBackupCodes] = useState<string[]>([]);
  const [passwordPolicy, setPasswordPolicy] = useState<PasswordPolicy | null>(
    null,
  );

  const [bindModalOpen, setBindModalOpen] = useState(false);
  const [bindPassword, setBindPassword] = useState("");
  const [bindPasswordError, setBindPasswordError] = useState<
    string | undefined
  >(undefined);
  const [regenConfirmOpen, setRegenConfirmOpen] = useState(false);
  const [disableConfirmOpen, setDisableConfirmOpen] = useState(false);

  const [passwordForm, setPasswordForm] = useState<ChangePasswordRequest>({
    current_password: "",
    new_password: "",
    confirm_password: "",
  });

  const drawerSubtitle = userInfo.username
    ? t("accountSecurity.drawer.subtitleWithUser", {
        username: userInfo.username,
      })
    : t("accountSecurity.drawer.subtitle");

  const mfaEnabled = Boolean(status?.enabled);
  const mfaRequired = Boolean(status?.mfa_required);
  const backupCodesRemaining = status?.backup_codes_remaining ?? 0;
  const changedAtText = status?.changed_at
    ? formatExpiryUtc8(status.changed_at) || "—"
    : "—";
  const setupDigits: 6 | 8 =
    (setupData?.totp_digits ?? status?.totp_digits ?? 6) === 8 ? 8 : 6;

  const hasPasswordInput =
    Boolean(passwordForm.current_password) ||
    Boolean(passwordForm.new_password) ||
    Boolean(passwordForm.confirm_password);

  const policySummaryLines = useMemo(() => {
    if (!passwordPolicy) return [];
    return formatPasswordPolicySummaryLines(passwordPolicy, t);
  }, [passwordPolicy, t]);

  const applyMfaSessionExpiry = useCallback(
    (payload: MfaSessionRefresh | void) => {
      if (!payload) return;
      const expiresIn = readSessionExpiresIn(payload);
      if (expiresIn != null) {
        setToken(payload.token, expiresIn);
      }
    },
    [setToken],
  );

  const loadStatus = useCallback(async () => {
    setStatusLoading(true);
    try {
      setStatus(await getMfaStatus());
    } catch (error) {
      logger.error("Failed to load MFA status:", error);
    } finally {
      setStatusLoading(false);
    }
  }, []);

  const loadPasswordPolicy = useCallback(async () => {
    try {
      const response = await fetchPasswordPolicy();
      setPasswordPolicy(resolvePasswordPolicyPayload(response));
    } catch (error) {
      logger.warn("Failed to load password policy summary:", error);
    }
  }, []);

  const refreshAfterMfaChange = useCallback(async () => {
    await Promise.all([
      loadStatus(),
      fetchGetUserInfo()
        .then((fresh) => setUserInfo(fresh))
        .catch((error) => {
          logger.warn("Failed to refresh user info after MFA change:", error);
        }),
      useMfaStepUpStore
        .getState()
        .refresh(true)
        .catch(() => undefined),
    ]);
  }, [loadStatus, setUserInfo]);

  const resetPasswordForm = useCallback(() => {
    setPasswordForm({
      current_password: "",
      new_password: "",
      confirm_password: "",
    });
  }, []);

  useEffect(() => {
    if (!open) {
      setSetupData(null);
      setSetupCode("");
      setBackupCodes([]);
      setBindModalOpen(false);
      setBindPassword("");
      setBindPasswordError(undefined);
      setRegenConfirmOpen(false);
      setDisableConfirmOpen(false);
      resetPasswordForm();
      return;
    }

    void loadStatus();
    void loadPasswordPolicy();
    void useMfaStepUpStore
      .getState()
      .refresh()
      .catch(() => undefined);

    const timer = setTimeout(() => {
      const target =
        focusSection === "password"
          ? passwordSectionRef.current
          : mfaSectionRef.current;
      target?.scrollIntoView({ behavior: "smooth", block: "start" });
    }, 80);
    return () => clearTimeout(timer);
  }, [focusSection, loadPasswordPolicy, loadStatus, open, resetPasswordForm]);

  const handleBindPasswordSubmit = async (event?: FormEvent) => {
    event?.preventDefault();
    if (!bindPassword.trim()) {
      setBindPasswordError(t("accountSecurity.setup.passwordRequired"));
      return;
    }
    setActionLoading(true);
    setBindPasswordError(undefined);
    try {
      const data = await getMfaSetup(bindPassword);
      setSetupData(data);
      setSetupCode("");
      setBackupCodes(data.backup_codes ?? []);
      setBindModalOpen(false);
      setBindPassword("");
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to start MFA setup:", error);
      }
    } finally {
      setActionLoading(false);
    }
  };

  const handleEnableMfa = async (event?: FormEvent) => {
    event?.preventDefault();
    const trimmed = setupCode.trim();
    if (!new RegExp(`^\\d{${setupDigits}}$`).test(trimmed)) {
      showWarningToast(
        t("accountSecurity.setup.codeInvalid", { digits: setupDigits }),
      );
      return;
    }
    setActionLoading(true);
    try {
      const enabled = await enableMfa({ totp_code: trimmed });
      applyMfaSessionExpiry(enabled);
      setSetupData(null);
      setSetupCode("");
      await refreshAfterMfaChange();
    } catch (error) {
      if (isWindowUnavailable(error)) {
        await refreshAfterMfaChange();
      }
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to enable MFA:", error);
      }
    } finally {
      setActionLoading(false);
    }
  };

  const handleRegenerateCodes = async () => {
    setRegenConfirmOpen(false);
    setActionLoading(true);
    try {
      const result = await regenerateBackupCodes();
      setBackupCodes(result.backup_codes ?? []);
      await loadStatus();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to regenerate backup codes:", error);
      }
    } finally {
      setActionLoading(false);
    }
  };

  const handleDisableMfa = async () => {
    if (!isAdmin || mfaRequired) return;
    setDisableConfirmOpen(false);
    setActionLoading(true);
    try {
      const refreshed = await disableMfa();
      applyMfaSessionExpiry(refreshed);
      setBackupCodes([]);
      await refreshAfterMfaChange();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to disable MFA:", error);
      }
    } finally {
      setActionLoading(false);
    }
  };

  const handleChangePassword = async (event?: FormEvent) => {
    event?.preventDefault();
    if (!passwordForm.current_password.trim()) {
      showWarningToast(t("accountSecurity.password.currentRequired"));
      return;
    }
    if (!passwordForm.new_password.trim()) {
      showWarningToast(t("accountSecurity.password.newRequired"));
      return;
    }
    if (!passwordForm.confirm_password.trim()) {
      showWarningToast(t("accountSecurity.password.confirmRequired"));
      return;
    }
    if (passwordForm.confirm_password !== passwordForm.new_password) {
      showWarningToast(t("accountSecurity.password.mismatch"));
      return;
    }

    setPasswordLoading(true);
    try {
      await changePassword({ ...passwordForm });
      resetPasswordForm();
    } catch (error) {
      logger.error("Failed to change password:", error);
    } finally {
      setPasswordLoading(false);
    }
  };

  return (
    <>
      <Drawer
        open={open}
        onClose={closeDrawer}
        size="md"
        title={t("accountSecurity.drawer.title")}
        subtitle={drawerSubtitle}
        testId="personal-security-drawer"
        footer={
          <Button type="button" variant="outline" onClick={closeDrawer}>
            {t("common.actions.close")}
          </Button>
        }
      >
        <div className="space-y-4">
          <div ref={mfaSectionRef}>
            <Card className="p-4 space-y-4">
              <div className="flex flex-wrap items-start justify-between gap-3 border-b border-border pb-3">
                <div className="min-w-0 space-y-1">
                  <div className="flex items-center gap-2">
                    <ShieldCheck className="h-4 w-4 text-xblue shrink-0" />
                    <h3 className="text-sm font-semibold text-foreground">
                      {t("accountSecurity.mfa.title")}
                    </h3>
                    <Badge variant={mfaEnabled ? "success" : "neutral"}>
                      {mfaEnabled
                        ? t("accountSecurity.mfa.statusEnabled")
                        : t("accountSecurity.mfa.statusDisabled")}
                    </Badge>
                  </div>
                  <p className="text-xs text-muted-foreground leading-relaxed">
                    {t("accountSecurity.mfa.hint")}
                  </p>
                </div>

                <div className="flex items-center gap-2 shrink-0">
                  {mfaEnabled ? (
                    <>
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        loading={actionLoading}
                        onClick={() => setRegenConfirmOpen(true)}
                        data-testid="security-mfa-regenerate-btn"
                      >
                        {t("accountSecurity.actions.regenerateCodes")}
                      </Button>
                      {isAdmin && !mfaRequired && (
                        <Button
                          type="button"
                          variant="danger"
                          size="sm"
                          loading={actionLoading}
                          onClick={() => setDisableConfirmOpen(true)}
                          data-testid="security-mfa-disable-btn"
                        >
                          {t("accountSecurity.actions.disable")}
                        </Button>
                      )}
                    </>
                  ) : setupData ? (
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      onClick={() => {
                        setSetupData(null);
                        setSetupCode("");
                        setBackupCodes([]);
                      }}
                    >
                      {t("common.cancel")}
                    </Button>
                  ) : (
                    <Button
                      type="button"
                      variant="primary"
                      size="sm"
                      loading={actionLoading || statusLoading}
                      onClick={() => {
                        setBindPassword("");
                        setBindPasswordError(undefined);
                        setBindModalOpen(true);
                      }}
                      data-testid="security-mfa-bind-btn"
                    >
                      {t("accountSecurity.actions.bind")}
                    </Button>
                  )}
                </div>
              </div>

              {mfaEnabled ? (
                <div className="space-y-2.5">
                  <div className="flex items-center justify-between gap-4 text-xs">
                    <span className="text-muted-foreground">
                      {t("accountSecurity.mfa.changedAt")}
                    </span>
                    <span className="font-mono-tech text-foreground">
                      {changedAtText}
                    </span>
                  </div>
                  <div className="flex items-center justify-between gap-4 text-xs">
                    <span className="text-muted-foreground">
                      {t("accountSecurity.mfa.backupCodesRemaining")}
                    </span>
                    <span
                      className={cn(
                        "font-mono-tech font-medium",
                        backupCodesRemaining <= 2
                          ? "text-amber-700 dark:text-amber-400"
                          : "text-foreground",
                      )}
                    >
                      {backupCodesRemaining}
                    </span>
                  </div>
                  {(mfaRequired || !isAdmin) && (
                    <p className="text-xs text-muted-foreground leading-relaxed pt-1">
                      {mfaRequired
                        ? t("accountSecurity.mfa.forcedHint")
                        : t("accountSecurity.mfa.userHint")}
                    </p>
                  )}
                </div>
              ) : !setupData ? (
                <div className="space-y-2.5">
                  <div className="flex items-center justify-between gap-4 text-xs">
                    <span className="text-muted-foreground">
                      {t("accountSecurity.mfa.specLabel")}
                    </span>
                    <span className="text-foreground">
                      {t("accountSecurity.mfa.specDigitsValue", {
                        digits: setupDigits,
                      })}
                    </span>
                  </div>
                  <div className="flex items-center justify-between gap-4 text-xs">
                    <span className="text-muted-foreground">
                      {t("accountSecurity.mfa.changedAt")}
                    </span>
                    <span className="font-mono-tech text-foreground">
                      {changedAtText}
                    </span>
                  </div>
                </div>
              ) : (
                <div className="flex flex-col sm:flex-row gap-4 items-start">
                  <div className="qr-surface inline-flex rounded-md p-2 border border-border shrink-0">
                    <QRCodeSVG
                      value={setupData.qr_code_url}
                      size={136}
                      level="M"
                      bgColor="#ffffff"
                      fgColor="#000000"
                    />
                  </div>
                  <div className="flex-1 min-w-0 w-full space-y-3">
                    <FormField label={t("accountSecurity.setup.secretLabel")}>
                      <div className="flex items-center gap-2">
                        <Input
                          value={setupData.secret}
                          readOnly
                          className="font-mono-tech"
                        />
                        <Button
                          type="button"
                          variant="outline"
                          onClick={() =>
                            void copyWithFeedback(setupData.secret)
                          }
                        >
                          <Copy className="h-3.5 w-3.5" />
                          {t("accountSecurity.actions.copy")}
                        </Button>
                      </div>
                    </FormField>

                    <form onSubmit={(e) => void handleEnableMfa(e)}>
                      <FormField
                        label={t("accountSecurity.setup.codeLabel", {
                          digits: setupDigits,
                        })}
                      >
                        <div className="flex items-center gap-2">
                          <Input
                            value={setupCode}
                            maxLength={setupDigits}
                            inputMode="numeric"
                            autoComplete="one-time-code"
                            placeholder={t(
                              "accountSecurity.setup.codePlaceholder",
                            )}
                            data-testid="security-mfa-setup-code"
                            onChange={(e) =>
                              setSetupCode(e.target.value.trim())
                            }
                          />
                          <Button
                            type="submit"
                            variant="primary"
                            loading={actionLoading}
                            data-testid="security-mfa-enable-btn"
                          >
                            {t("accountSecurity.actions.enable")}
                          </Button>
                        </div>
                      </FormField>
                    </form>
                  </div>
                </div>
              )}

              {backupCodes.length > 0 && (
                <div className="border-t border-border pt-3 space-y-2.5">
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-xs font-medium text-foreground">
                      {t("accountSecurity.backupCodes.title")}
                    </span>
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      onClick={() =>
                        void copyWithFeedback(backupCodes.join("\n"))
                      }
                    >
                      <Copy className="h-3.5 w-3.5" />
                      {t("accountSecurity.actions.copyAll")}
                    </Button>
                  </div>
                  <p className="text-xs text-muted-foreground leading-relaxed">
                    {t("accountSecurity.backupCodes.hint")}
                  </p>
                  <div className="grid grid-cols-2 sm:grid-cols-4 gap-2 rounded-md border border-border bg-muted/40 p-2.5">
                    {backupCodes.map((code) => (
                      <code
                        key={code}
                        className="rounded border border-border bg-card px-2 py-1 text-center font-mono-tech text-xs text-foreground"
                      >
                        {code}
                      </code>
                    ))}
                  </div>
                </div>
              )}
            </Card>
          </div>

          <div ref={passwordSectionRef}>
            <Card className="p-4 space-y-4">
              <div className="border-b border-border pb-3 space-y-1.5">
                <div className="flex items-center gap-2">
                  <KeyRound className="h-4 w-4 text-xblue shrink-0" />
                  <h3 className="text-sm font-semibold text-foreground">
                    {t("accountSecurity.password.title")}
                  </h3>
                </div>
                {policySummaryLines.length > 0 ? (
                  <ul className="space-y-0.5 text-xs text-muted-foreground">
                    {policySummaryLines.map((line) => (
                      <li key={line}>{line}</li>
                    ))}
                  </ul>
                ) : (
                  <p className="text-xs text-muted-foreground">
                    {t("accountSecurity.password.policyHint")}
                  </p>
                )}
              </div>

              <form
                onSubmit={(e) => void handleChangePassword(e)}
                className="space-y-3"
              >
                <input
                  type="text"
                  name="username"
                  value={userInfo.username ?? ""}
                  autoComplete="username"
                  readOnly
                  tabIndex={-1}
                  aria-hidden="true"
                  className="hidden"
                />
                <FormField
                  label={t("accountSecurity.password.current")}
                  htmlFor="security-current-password"
                  required
                >
                  <PasswordInput
                    id="security-current-password"
                    name="current_password"
                    value={passwordForm.current_password}
                    autoComplete="current-password"
                    data-testid="security-current-password"
                    onChange={(e) =>
                      setPasswordForm((prev) => ({
                        ...prev,
                        current_password: e.target.value,
                      }))
                    }
                  />
                </FormField>

                <FormField
                  label={t("accountSecurity.password.new")}
                  htmlFor="security-new-password"
                  required
                >
                  <PasswordInput
                    id="security-new-password"
                    name="new_password"
                    value={passwordForm.new_password}
                    autoComplete="new-password"
                    data-testid="security-new-password"
                    onChange={(e) =>
                      setPasswordForm((prev) => ({
                        ...prev,
                        new_password: e.target.value,
                      }))
                    }
                  />
                </FormField>

                <FormField
                  label={t("accountSecurity.password.confirm")}
                  htmlFor="security-confirm-password"
                  required
                >
                  <PasswordInput
                    id="security-confirm-password"
                    name="confirm_password"
                    value={passwordForm.confirm_password}
                    autoComplete="new-password"
                    data-testid="security-confirm-password"
                    onChange={(e) =>
                      setPasswordForm((prev) => ({
                        ...prev,
                        confirm_password: e.target.value,
                      }))
                    }
                  />
                </FormField>

                <div className="flex items-center justify-end gap-2 pt-1">
                  {hasPasswordInput && (
                    <Button
                      type="button"
                      variant="outline"
                      disabled={passwordLoading}
                      onClick={resetPasswordForm}
                    >
                      {t("common.reset")}
                    </Button>
                  )}
                  <Button
                    type="submit"
                    variant="primary"
                    loading={passwordLoading}
                    data-testid="security-change-password-submit"
                  >
                    {t("accountSecurity.actions.confirmChangePassword")}
                  </Button>
                </div>
              </form>
            </Card>
          </div>
        </div>
      </Drawer>

      <Modal
        open={bindModalOpen}
        onClose={() => {
          if (!actionLoading) setBindModalOpen(false);
        }}
        size="sm"
        closeOnBackdrop={false}
        testId="mfa-bind-password-modal"
        title={t("accountSecurity.setup.passwordTitle")}
        footer={
          <>
            <Button
              type="button"
              variant="outline"
              disabled={actionLoading}
              onClick={() => setBindModalOpen(false)}
            >
              {t("common.cancel")}
            </Button>
            <Button
              type="button"
              variant="primary"
              loading={actionLoading}
              onClick={() => void handleBindPasswordSubmit()}
              data-testid="mfa-bind-password-confirm"
            >
              {t("common.confirm")}
            </Button>
          </>
        }
      >
        <form
          onSubmit={(e) => void handleBindPasswordSubmit(e)}
          className="space-y-3"
        >
          <p className="text-xs text-muted-foreground leading-relaxed">
            {t("accountSecurity.setup.passwordMessage")}
          </p>
          <FormField
            label={t("accountSecurity.setup.passwordPlaceholder")}
            error={bindPasswordError}
            required
          >
            <PasswordInput
              value={bindPassword}
              autoFocus
              autoComplete="current-password"
              placeholder={t("accountSecurity.setup.passwordPlaceholder")}
              data-testid="mfa-bind-password-input"
              onChange={(e) => {
                setBindPassword(e.target.value);
                if (bindPasswordError) setBindPasswordError(undefined);
              }}
            />
          </FormField>
        </form>
      </Modal>

      <ConfirmDialog
        open={regenConfirmOpen}
        onClose={() => setRegenConfirmOpen(false)}
        onConfirm={() => void handleRegenerateCodes()}
        title={t("accountSecurity.actions.regenerateCodes")}
        description={t("accountSecurity.backupCodes.regenerateConfirm")}
        confirmLabel={t("common.confirm")}
        cancelLabel={t("common.cancel")}
        danger={false}
        loading={actionLoading}
      />

      <ConfirmDialog
        open={disableConfirmOpen}
        onClose={() => setDisableConfirmOpen(false)}
        onConfirm={() => void handleDisableMfa()}
        title={t("accountSecurity.disable.title")}
        description={t("accountSecurity.disable.message")}
        confirmLabel={t("accountSecurity.actions.disable")}
        cancelLabel={t("common.cancel")}
        danger
        loading={actionLoading}
      />
    </>
  );
}
