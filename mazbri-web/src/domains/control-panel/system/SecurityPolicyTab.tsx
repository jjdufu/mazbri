import { useCallback, useEffect, useMemo, useState } from "react";
import {
  History,
  Loader2,
  Lock,
  Save,
  ShieldCheck,
  Smartphone,
} from "lucide-react";
import {
  fetchMfaSettings,
  fetchPasswordPolicy,
  updateMfaSettings,
  updatePasswordPolicy,
  type MfaSettings,
  type UpdatePasswordPolicyRequest,
} from "@/api/audit";
import {
  Button,
  Input,
  NumberInput,
  SettingRow,
  SettingsSectionCard,
  SettingsSectionLayout,
  Switch,
} from "@/components/ui";
import { useT } from "@/locales";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { resolvePasswordPolicyPayload } from "@/utils/password";
import { logger } from "@/utils/sys/logger";
import {
  showSuccessToast,
  showWarningToast,
} from "@/utils/ui/message-guard";

interface PasswordPolicyFormState {
  min_length: number;
  max_length: number;
  require_uppercase: boolean;
  require_lowercase: boolean;
  require_digit: boolean;
  require_special_char: boolean;
  special_chars: string;
  max_age_days: number;
  warning_days: number;
  prevent_reuse_count: number;
  max_login_attempts: number;
  lockout_duration_minutes: number;
}

const DEFAULT_POLICY: PasswordPolicyFormState = {
  min_length: 8,
  max_length: 128,
  require_uppercase: true,
  require_lowercase: true,
  require_digit: true,
  require_special_char: false,
  special_chars: "!@#$%^&*()_+-=",
  max_age_days: 90,
  warning_days: 7,
  prevent_reuse_count: 5,
  max_login_attempts: 5,
  lockout_duration_minutes: 30,
};

const DEFAULT_MFA: MfaSettings = {
  totp_issuer: "Mazbri",
  totp_digits: 6,
  totp_period: 30,
  backup_codes_count: 10,
};

export interface SecurityPolicyTabProps {
  onDirtyChange: (dirty: boolean) => void;
}

export function SecurityPolicyTab({ onDirtyChange }: SecurityPolicyTabProps) {
  const { t } = useT();
  const [loading, setLoading] = useState(false);

  // Domain 1: Password & Login Protection Policy
  const [policyForm, setPolicyForm] =
    useState<PasswordPolicyFormState>(DEFAULT_POLICY);
  const [policyBaseline, setPolicyBaseline] =
    useState<PasswordPolicyFormState | null>(null);
  const [savingPolicy, setSavingPolicy] = useState(false);

  // Domain 2 (U4 Independent Step-Up Boundary): Global MFA Settings
  const [mfaForm, setMfaForm] = useState<MfaSettings>(DEFAULT_MFA);
  const [mfaBaseline, setMfaBaseline] = useState<string | null>(null);
  const [savingMfa, setSavingMfa] = useState(false);

  const loadAll = useCallback(async () => {
    setLoading(true);
    try {
      const [policyRes, mfaRes] = await Promise.all([
        fetchPasswordPolicy(),
        fetchMfaSettings(),
      ]);
      const policy = resolvePasswordPolicyPayload(policyRes);
      if (policy) {
        const nextPolicy: PasswordPolicyFormState = {
          min_length: policy.min_length,
          max_length: policy.max_length,
          require_uppercase: policy.require_uppercase,
          require_lowercase: policy.require_lowercase,
          require_digit: policy.require_digit,
          require_special_char: policy.require_special_char,
          special_chars: policy.special_chars,
          max_age_days: policy.max_age_days,
          warning_days: policy.warning_days,
          prevent_reuse_count: policy.prevent_reuse_count,
          max_login_attempts: policy.max_login_attempts,
          lockout_duration_minutes: policy.lockout_duration_minutes,
        };
        setPolicyForm(nextPolicy);
        setPolicyBaseline(nextPolicy);
      }
      const mfaData = mfaRes?.data;
      if (mfaData) {
        const nextMfa: MfaSettings = {
          totp_issuer: mfaData.totp_issuer || "Mazbri",
          totp_digits: mfaData.totp_digits || 6,
          totp_period: mfaData.totp_period || 30,
          backup_codes_count: mfaData.backup_codes_count || 10,
        };
        setMfaForm(nextMfa);
        setMfaBaseline(nextMfa.totp_issuer);
      }
    } catch (error) {
      logger.error("Failed to load platform security policy:", error);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadAll();
  }, [loadAll]);

  const isPolicyDirty = useMemo(
    () =>
      policyBaseline !== null &&
      JSON.stringify(policyForm) !== JSON.stringify(policyBaseline),
    [policyForm, policyBaseline],
  );

  const isMfaDirty = useMemo(
    () => mfaBaseline !== null && mfaForm.totp_issuer !== mfaBaseline,
    [mfaForm.totp_issuer, mfaBaseline],
  );

  useEffect(() => {
    onDirtyChange(isPolicyDirty || isMfaDirty);
    return () => onDirtyChange(false);
  }, [isPolicyDirty, isMfaDirty, onDirtyChange]);

  const handleSavePasswordPolicy = async () => {
    if (savingPolicy) return;
    if (
      policyForm.require_special_char &&
      !policyForm.special_chars.trim()
    ) {
      showWarningToast(t("securityPolicy.messages.specialCharsRequired"));
      return;
    }

    setSavingPolicy(true);
    try {
      const payload: UpdatePasswordPolicyRequest = {
        min_length: policyForm.min_length,
        max_length: policyForm.max_length,
        require_uppercase: policyForm.require_uppercase,
        require_lowercase: policyForm.require_lowercase,
        require_digit: policyForm.require_digit,
        require_special_char: policyForm.require_special_char,
        special_chars: policyForm.special_chars.trim(),
        max_age_days: policyForm.max_age_days,
        warning_days: policyForm.warning_days,
        prevent_reuse_count: policyForm.prevent_reuse_count,
        max_login_attempts: policyForm.max_login_attempts,
        lockout_duration_minutes: policyForm.lockout_duration_minutes,
      };
      await updatePasswordPolicy(payload);
      const nextBaseline = {
        ...policyForm,
        special_chars: policyForm.special_chars.trim(),
      };
      setPolicyForm(nextBaseline);
      setPolicyBaseline(nextBaseline);
      showSuccessToast(t("securityPolicy.messages.savedSuccess"));
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to save password policy:", error);
      }
    } finally {
      setSavingPolicy(false);
    }
  };

  /** U4: Independent MFA privilege domain — NEVER merged into password policy. */
  const handleSaveMfaSettings = async () => {
    if (savingMfa) return;
    const issuer = mfaForm.totp_issuer.trim();
    if (!issuer) {
      showWarningToast(t("securityPolicy.messages.issuerRequired"));
      return;
    }
    setSavingMfa(true);
    try {
      await updateMfaSettings({ totp_issuer: issuer });
      setMfaForm((prev) => ({ ...prev, totp_issuer: issuer }));
      setMfaBaseline(issuer);
      showSuccessToast(t("common.feedback.saveSuccess"));
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to save global MFA settings:", error);
      }
    } finally {
      setSavingMfa(false);
    }
  };

  if (loading && !policyBaseline && mfaBaseline === null) {
    return (
      <div className="flex items-center justify-center py-16 text-xs text-muted-foreground gap-2">
        <Loader2 className="h-4 w-4 animate-spin text-primary" />
        <span>{t("layout.loading")}</span>
      </div>
    );
  }

  return (
    <SettingsSectionLayout
      testId="security-policy-tab"
      columns={2}
      dirty={isPolicyDirty}
      saving={savingPolicy}
      onSave={() => void handleSavePasswordPolicy()}
      onReset={() => {
        if (policyBaseline) setPolicyForm(policyBaseline);
      }}
      dirtyLabel={t("systemSettings.unsavedChanges")}
      saveLabel={t("common.actions.save")}
      resetLabel={t("common.reset")}
    >
      {/* 1. Password Complexity */}
      <SettingsSectionCard
        title={t("securityPolicy.sections.complexity.title")}
        icon={<ShieldCheck className="h-4 w-4" />}
      >
        <SettingRow
          label={t("securityPolicy.fields.minLength")}
          layout="inline"
        >
          <NumberInput
            value={policyForm.min_length}
            min={6}
            max={32}
            onChange={(next) =>
              setPolicyForm((prev) => ({ ...prev, min_length: next }))
            }
            aria-label={t("securityPolicy.fields.minLength")}
            className="w-32"
          />
        </SettingRow>

        <SettingRow
          label={t("securityPolicy.fields.maxLength")}
          layout="inline"
        >
          <NumberInput
            value={policyForm.max_length}
            min={32}
            max={256}
            onChange={(next) =>
              setPolicyForm((prev) => ({ ...prev, max_length: next }))
            }
            aria-label={t("securityPolicy.fields.maxLength")}
            className="w-32"
          />
        </SettingRow>

        <SettingRow
          label={t("securityPolicy.fields.specialChars")}
          htmlFor="policy-special-chars"
          hint={t("securityPolicy.hints.specialChars")}
        >
          <Input
            id="policy-special-chars"
            value={policyForm.special_chars}
            disabled={!policyForm.require_special_char}
            onChange={(e) =>
              setPolicyForm((prev) => ({
                ...prev,
                special_chars: e.target.value,
              }))
            }
            placeholder="!@#$%^&*()_+-="
          />
        </SettingRow>

        <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
          {(
            [
              [
                "require_uppercase",
                "securityPolicy.fields.requireUppercase",
              ],
              [
                "require_lowercase",
                "securityPolicy.fields.requireLowercase",
              ],
              ["require_digit", "securityPolicy.fields.requireDigit"],
              [
                "require_special_char",
                "securityPolicy.fields.requireSpecialChar",
              ],
            ] as const
          ).map(([field, labelKey]) => (
            <div
              key={field}
              className="flex items-center justify-between gap-2 rounded-md border border-border bg-card px-3 py-2"
            >
              <span className="text-xs text-foreground">{t(labelKey)}</span>
              <Switch
                checked={policyForm[field]}
                onCheckedChange={(checked) =>
                  setPolicyForm((prev) => ({ ...prev, [field]: checked }))
                }
                aria-label={t(labelKey)}
              />
            </div>
          ))}
        </div>
      </SettingsSectionCard>

      {/* 2. Password Expiry & Reuse */}
      <SettingsSectionCard
        title={t("securityPolicy.sections.expiration.title")}
        icon={<History className="h-4 w-4" />}
      >
        <SettingRow
          label={t("securityPolicy.fields.passwordValidity")}
          layout="inline"
        >
          <NumberInput
            value={policyForm.max_age_days}
            min={0}
            max={365}
            onChange={(next) =>
              setPolicyForm((prev) => ({ ...prev, max_age_days: next }))
            }
            aria-label={t("securityPolicy.fields.passwordValidity")}
            className="w-32"
          />
        </SettingRow>

        <SettingRow
          label={t("securityPolicy.fields.expirationWarning")}
          layout="inline"
        >
          <NumberInput
            value={policyForm.warning_days}
            min={0}
            max={30}
            onChange={(next) =>
              setPolicyForm((prev) => ({ ...prev, warning_days: next }))
            }
            aria-label={t("securityPolicy.fields.expirationWarning")}
            className="w-32"
          />
        </SettingRow>

        <SettingRow
          label={t("securityPolicy.fields.historyCheck")}
          layout="inline"
          hint={t("securityPolicy.hints.expiration")}
        >
          <NumberInput
            value={policyForm.prevent_reuse_count}
            min={0}
            max={24}
            onChange={(next) =>
              setPolicyForm((prev) => ({ ...prev, prevent_reuse_count: next }))
            }
            aria-label={t("securityPolicy.fields.historyCheck")}
            className="w-32"
          />
        </SettingRow>
      </SettingsSectionCard>

      {/* 3. Login Protection */}
      <SettingsSectionCard
        title={t("securityPolicy.sections.loginProtection.title")}
        icon={<Lock className="h-4 w-4" />}
      >
        <SettingRow
          label={t("securityPolicy.fields.lockThreshold")}
          layout="inline"
        >
          <NumberInput
            value={policyForm.max_login_attempts}
            min={3}
            max={10}
            onChange={(next) =>
              setPolicyForm((prev) => ({ ...prev, max_login_attempts: next }))
            }
            aria-label={t("securityPolicy.fields.lockThreshold")}
            className="w-32"
          />
        </SettingRow>

        <SettingRow
          label={t("securityPolicy.fields.lockDuration")}
          layout="inline"
          hint={t("securityPolicy.hints.lockThreshold")}
        >
          <NumberInput
            value={policyForm.lockout_duration_minutes}
            min={5}
            max={1440}
            onChange={(next) =>
              setPolicyForm((prev) => ({
                ...prev,
                lockout_duration_minutes: next,
              }))
            }
            aria-label={t("securityPolicy.fields.lockDuration")}
            className="w-32"
          />
        </SettingRow>
      </SettingsSectionCard>

      {/* 4. Global MFA Card (U4 Independent Step-Up & Save Boundary) */}
      <SettingsSectionCard
        testId="global-mfa-card"
        title={t("securityPolicy.sections.totp.globalTitle")}
        icon={<Smartphone className="h-4 w-4" />}
        actions={
          <Button
            type="button"
            variant="primary"
            size="sm"
            loading={savingMfa}
            disabled={!isMfaDirty}
            onClick={() => void handleSaveMfaSettings()}
            data-testid="global-mfa-save-btn"
          >
            <Save className="h-3.5 w-3.5" />
            {t("common.actions.save")}
          </Button>
        }
      >
        <SettingRow
          label={t("securityPolicy.fields.issuerName")}
          htmlFor="global-mfa-issuer"
          hint={t("securityPolicy.hints.totpIssuer")}
        >
          <Input
            id="global-mfa-issuer"
            value={mfaForm.totp_issuer}
            disabled={savingMfa}
            onChange={(e) =>
              setMfaForm((prev) => ({ ...prev, totp_issuer: e.target.value }))
            }
            placeholder="Mazbri"
          />
        </SettingRow>

        <SettingRow
          label={t("accountSecurity.mfa.specLabel")}
          layout="inline"
        >
          <span className="text-xs font-mono-tech text-muted-foreground tabular-nums">
            TOTP {mfaForm.totp_digits} {t("securityPolicy.units.digits")} ·{" "}
            {mfaForm.totp_period}s · {mfaForm.backup_codes_count}{" "}
            {t("securityPolicy.units.backupCodes")}
          </span>
        </SettingRow>
      </SettingsSectionCard>
    </SettingsSectionLayout>
  );
}
