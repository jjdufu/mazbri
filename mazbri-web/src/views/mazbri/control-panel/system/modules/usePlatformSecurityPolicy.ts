import { inject, provide, reactive, ref, toRaw } from "vue";
import type { InjectionKey, Ref } from "vue";
import { ElMessage } from "element-plus";
import { runWithLoading } from "@/composables/useAsyncState";
import {
  fetchMfaSettings,
  fetchPasswordPolicy,
  updateMfaSettings,
  updatePasswordPolicy,
  type MfaSettings,
  type UpdatePasswordPolicyRequest,
} from "@/api/audit";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-dialog-service";
import { resolvePasswordPolicyPayload } from "@/utils/password";
import { $t } from "@/locales";
import { logger } from "@/utils/sys";

export interface PlatformSecurityPolicyState {
  policyForm: UpdatePasswordPolicyRequest;
  mfaForm: MfaSettings;
  loading: Ref<boolean>;
  saving: Ref<boolean>;
  issuerSaving: Ref<boolean>;
  loadAll: () => Promise<void>;
  savePolicy: () => Promise<void>;
  saveIssuer: () => Promise<void>;
}

const PLATFORM_SECURITY_POLICY_KEY: InjectionKey<PlatformSecurityPolicyState> =
  Symbol("platform-security-policy");

function createPlatformSecurityPolicy(): PlatformSecurityPolicyState {
  const loading = ref(false);
  const saving = ref(false);
  const issuerSaving = ref(false);

  const policyForm = reactive<UpdatePasswordPolicyRequest>({
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
  });

  const mfaForm = reactive<MfaSettings>({
    totp_issuer: "Mazbri",
    totp_digits: 6,
    totp_period: 30,
    backup_codes_count: 10,
  });

  async function loadPolicy(): Promise<void> {
    const response = await fetchPasswordPolicy();
    const policy = resolvePasswordPolicyPayload(response);
    if (!policy) return;
    Object.assign(policyForm, {
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
    });
  }

  async function loadMfa(): Promise<void> {
    const res = await fetchMfaSettings();
    const data = res.data;
    mfaForm.totp_issuer = data.totp_issuer || "Mazbri";
    mfaForm.totp_digits = data.totp_digits || 6;
    mfaForm.totp_period = data.totp_period || 30;
    mfaForm.backup_codes_count = data.backup_codes_count || 10;
  }

  async function loadAll(): Promise<void> {
    await runWithLoading(loading, async () => {
      await Promise.all([loadPolicy(), loadMfa()]);
    }).catch((error) => {
      logger.error("Failed to load platform security policy:", error);
    });
  }

  async function savePolicy(): Promise<void> {
    if (
      policyForm.require_special_char &&
      !(policyForm.special_chars || "").trim()
    ) {
      ElMessage.warning($t("securityPolicy.messages.specialCharsRequired"));
      return;
    }
    await runWithLoading(saving, async () => {
      const policyPayload: UpdatePasswordPolicyRequest = {
        ...toRaw(policyForm),
        special_chars: (policyForm.special_chars || "").trim(),
      };
      await updatePasswordPolicy(policyPayload);
      ElMessage.success($t("securityPolicy.messages.savedSuccess"));
    }).catch((error) => {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to save platform security policy:", error);
      }
    });
  }

  async function saveIssuer(): Promise<void> {
    const issuer = mfaForm.totp_issuer.trim();
    if (!issuer) {
      ElMessage.warning($t("securityPolicy.messages.issuerRequired"));
      return;
    }
    await runWithLoading(issuerSaving, async () => {
      const updated = await updateMfaSettings({ totp_issuer: issuer });
      if (updated?.data?.totp_issuer) {
        mfaForm.totp_issuer = updated.data.totp_issuer;
      } else {
        mfaForm.totp_issuer = issuer;
      }
      ElMessage.success($t("securityPolicy.messages.savedSuccess"));
    }).catch((error) => {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to update global MFA issuer:", error);
      }
    });
  }

  return {
    policyForm,
    mfaForm,
    loading,
    saving,
    issuerSaving,
    loadAll,
    savePolicy,
    saveIssuer,
  };
}

export function providePlatformSecurityPolicy(): PlatformSecurityPolicyState {
  const state = createPlatformSecurityPolicy();
  provide(PLATFORM_SECURITY_POLICY_KEY, state);
  return state;
}

export function usePlatformSecurityPolicy(): PlatformSecurityPolicyState {
  const state = inject(PLATFORM_SECURITY_POLICY_KEY);
  if (!state) {
    throw new Error(
      "usePlatformSecurityPolicy must be used under providePlatformSecurityPolicy",
    );
  }
  return state;
}
