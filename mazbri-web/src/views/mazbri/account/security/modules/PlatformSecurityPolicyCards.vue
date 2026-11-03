<template>
  <div v-loading="loading" class="platform-security-policy">
    <div class="platform-security-policy__toolbar">
      <CommonInlineHint v-if="!canEdit" block size="xs">
        {{ $t("accountSecurity.platformPolicy.readOnlyHint") }}
      </CommonInlineHint>
      <template v-else>
        <MfaRequiredHint block />
        <ElButton type="primary" :loading="saving" @click="handleSaveAll">
          <template #icon><ArtSvgIcon icon="ri:save-line" /></template>
          {{ $t("securityPolicy.actions.saveSettings") }}
        </ElButton>
      </template>
    </div>

    <div class="security-grid">
      <ElCard shadow="hover" class="security-card">
        <template #header>
          <SurfaceCardHeader>
            <template #left>
              <div class="card-header-title">
                <ArtSvgIcon
                  icon="ri:shield-keyhole-line"
                  class="card-header-icon"
                />
                <span>{{ $t("securityPolicy.sections.complexity.title") }}</span>
              </div>
            </template>
          </SurfaceCardHeader>
        </template>
        <div class="settings-panel">
          <CommonSettingRow
            mode="inline"
            mobile-stack
            :label="$t('securityPolicy.fields.minLength')"
          >
            <ElInputNumber
              v-model="editForm.min_length"
              :min="6"
              :max="32"
              :disabled="!canEdit"
              class="setting-input-number"
            />
          </CommonSettingRow>
          <CommonSettingRow
            mode="inline"
            mobile-stack
            :label="$t('securityPolicy.fields.maxLength')"
          >
            <ElInputNumber
              v-model="editForm.max_length"
              :min="32"
              :max="256"
              :disabled="!canEdit"
              class="setting-input-number"
            />
          </CommonSettingRow>
          <CommonSettingRow
            mode="inline"
            mobile-stack
            :label="$t('securityPolicy.fields.requireUppercase')"
          >
            <ElSwitch
              v-model="editForm.require_uppercase"
              :disabled="!canEdit"
            />
          </CommonSettingRow>
          <CommonSettingRow
            mode="inline"
            mobile-stack
            :label="$t('securityPolicy.fields.requireLowercase')"
          >
            <ElSwitch
              v-model="editForm.require_lowercase"
              :disabled="!canEdit"
            />
          </CommonSettingRow>
          <CommonSettingRow
            mode="inline"
            mobile-stack
            :label="$t('securityPolicy.fields.requireDigit')"
          >
            <ElSwitch v-model="editForm.require_digit" :disabled="!canEdit" />
          </CommonSettingRow>
          <CommonSettingRow
            mode="inline"
            mobile-stack
            :label="$t('securityPolicy.fields.requireSpecialChar')"
          >
            <ElSwitch
              v-model="editForm.require_special_char"
              :disabled="!canEdit"
            />
          </CommonSettingRow>
          <CommonSettingRow
            mode="inline"
            mobile-stack
            :label="$t('securityPolicy.fields.specialChars')"
          >
            <ExpandOnFocusInput
              v-model="editForm.special_chars"
              :placeholder="$t('securityPolicy.placeholders.specialChars')"
              :compact-width="160"
              :expand-width="320"
              :disabled="!canEdit || !editForm.require_special_char"
            />
          </CommonSettingRow>
          <CommonInlineHint
            v-if="editForm.require_special_char"
            block
            size="xs"
          >
            {{ $t("securityPolicy.hints.specialChars") }}
          </CommonInlineHint>
        </div>
      </ElCard>

      <ElCard shadow="hover" class="security-card">
        <template #header>
          <SurfaceCardHeader>
            <template #left>
              <div class="card-header-title">
                <ArtSvgIcon
                  icon="ri:calendar-event-line"
                  class="card-header-icon"
                />
                <span>{{
                  $t("securityPolicy.sections.expiration.title")
                }}</span>
              </div>
            </template>
          </SurfaceCardHeader>
        </template>
        <div class="settings-panel">
          <CommonSettingRow
            mode="inline"
            mobile-stack
            :label="$t('securityPolicy.fields.passwordValidity')"
          >
            <ElInputNumber
              v-model="editForm.max_age_days"
              :min="0"
              :max="365"
              :disabled="!canEdit"
              class="setting-input-number"
            />
          </CommonSettingRow>
          <CommonSettingRow
            mode="inline"
            mobile-stack
            :label="$t('securityPolicy.fields.expirationWarning')"
          >
            <ElInputNumber
              v-model="editForm.warning_days"
              :min="0"
              :max="30"
              :disabled="!canEdit"
              class="setting-input-number"
            />
          </CommonSettingRow>
          <CommonSettingRow
            mode="inline"
            mobile-stack
            :label="$t('securityPolicy.fields.historyCheck')"
          >
            <ElInputNumber
              v-model="editForm.prevent_reuse_count"
              :min="0"
              :max="24"
              :disabled="!canEdit"
              class="setting-input-number"
            />
          </CommonSettingRow>
        </div>
      </ElCard>

      <ElCard shadow="hover" class="security-card">
        <template #header>
          <SurfaceCardHeader>
            <template #left>
              <div class="card-header-title">
                <ArtSvgIcon
                  icon="ri:shield-flash-line"
                  class="card-header-icon"
                />
                <span>{{
                  $t("securityPolicy.sections.loginProtection.title")
                }}</span>
              </div>
            </template>
          </SurfaceCardHeader>
        </template>
        <div class="settings-panel">
          <CommonSettingRow
            mode="inline"
            mobile-stack
            :label="$t('securityPolicy.fields.lockThreshold')"
          >
            <ElInputNumber
              v-model="editForm.max_login_attempts"
              :min="3"
              :max="10"
              :disabled="!canEdit"
              class="setting-input-number"
            />
          </CommonSettingRow>
          <CommonSettingRow
            mode="inline"
            mobile-stack
            :label="$t('securityPolicy.fields.lockDuration')"
          >
            <ElInputNumber
              v-model="editForm.lockout_duration_minutes"
              :min="5"
              :max="1440"
              :disabled="!canEdit"
              class="setting-input-number"
            />
          </CommonSettingRow>
        </div>
      </ElCard>

      <ElCard shadow="hover" class="security-card">
        <template #header>
          <SurfaceCardHeader>
            <template #left>
              <div class="card-header-title">
                <ArtSvgIcon icon="ri:key-2-line" class="card-header-icon" />
                <span>{{ $t("securityPolicy.sections.totp.title") }}</span>
              </div>
            </template>
          </SurfaceCardHeader>
        </template>
        <div class="settings-panel">
          <CommonSettingRow
            mode="inline"
            mobile-stack
            :label="$t('securityPolicy.fields.issuerName')"
          >
            <ExpandOnFocusInput
              v-model="mfaForm.totp_issuer"
              :placeholder="$t('securityPolicy.placeholders.issuerName')"
              :compact-width="120"
              :disabled="!canEdit"
            />
          </CommonSettingRow>
          <CommonSettingRow
            mode="inline"
            mobile-stack
            :label="$t('securityPolicy.fields.totpDigits')"
          >
            <ElSelect
              v-model="mfaForm.totp_digits"
              :disabled="!canEdit"
              class="setting-select"
            >
              <ElOption
                :label="$t('securityPolicy.options.totpDigits6')"
                :value="6"
              />
              <ElOption
                :label="$t('securityPolicy.options.totpDigits8')"
                :value="8"
              />
            </ElSelect>
          </CommonSettingRow>
          <CommonSettingRow
            mode="inline"
            mobile-stack
            :label="$t('securityPolicy.fields.totpPeriod')"
          >
            <ElSelect
              v-model="mfaForm.totp_period"
              :disabled="!canEdit"
              class="setting-select"
            >
              <ElOption
                :label="$t('securityPolicy.options.totpPeriod30')"
                :value="30"
              />
              <ElOption
                :label="$t('securityPolicy.options.totpPeriod60')"
                :value="60"
              />
            </ElSelect>
          </CommonSettingRow>
          <CommonInlineHint block size="xs" tone="warning">
            {{ $t("securityPolicy.sections.totp.digitsPeriodWarning") }}
          </CommonInlineHint>
          <CommonSettingRow
            mode="inline"
            mobile-stack
            :label="$t('securityPolicy.fields.backupCodesCount')"
          >
            <ElInputNumber
              v-model="mfaForm.backup_codes_count"
              :min="5"
              :max="20"
              :step="1"
              :disabled="!canEdit"
              class="setting-input-number"
            />
          </CommonSettingRow>
        </div>
      </ElCard>
    </div>
  </div>
</template>

<script setup lang="ts">
import { reactive, ref, toRaw, onMounted, onActivated } from "vue";
import { ElMessage } from "element-plus";
import SurfaceCardHeader from "@/components/common/surface-card-header/index.vue";
import CommonSettingRow from "@/components/common/setting-row/index.vue";
import CommonInlineHint from "@/components/common/inline-hint/index.vue";
import ExpandOnFocusInput from "@/components/common/expand-on-focus-input/index.vue";
import MfaRequiredHint from "@/components/common/mfa-required-hint/index.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { useOpsRole } from "@/composables/ops/useOpsRole";
import {
  fetchPasswordPolicy,
  updatePasswordPolicy,
  fetchMfaSettings,
  updateMfaSettings,
  type UpdatePasswordPolicyRequest,
  type UpdateMfaSettingsRequest,
  type MfaSettings,
} from "@/api/audit";
import { unwrapResponsePayload } from "@/utils/http/response";
import { resolvePasswordPolicyPayload } from "@/utils/password";
import { $t } from "@/locales";
import { logger } from "@/utils/sys";

defineOptions({ name: "PlatformSecurityPolicyCards" });

const { isAdmin } = useOpsRole();
const canEdit = isAdmin;
const loading = ref(false);
const saving = ref(false);

const editForm = reactive<UpdatePasswordPolicyRequest>({
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
  Object.assign(editForm, {
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
  const data = unwrapResponsePayload<MfaSettings>(res);
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

async function handleSaveAll(): Promise<void> {
  if (!canEdit.value) return;
  if (
    editForm.require_special_char &&
    !(editForm.special_chars || "").trim()
  ) {
    ElMessage.warning($t("securityPolicy.messages.specialCharsRequired"));
    return;
  }
  await runWithLoading(saving, async () => {
    const mfaPayload: UpdateMfaSettingsRequest = {
      totp_issuer: mfaForm.totp_issuer,
      totp_digits: mfaForm.totp_digits,
      totp_period: mfaForm.totp_period,
      backup_codes_count: mfaForm.backup_codes_count,
    };
    const policyPayload = {
      ...toRaw(editForm),
      special_chars: (editForm.special_chars || "").trim(),
    };
    await Promise.all([
      updatePasswordPolicy(policyPayload),
      updateMfaSettings(mfaPayload),
    ]);
    ElMessage.success($t("securityPolicy.messages.savedSuccess"));
  }).catch((error) => {
    logger.error("Failed to save platform security policy:", error);
  });
}

onMounted(() => {
  void loadAll();
});

onActivated(() => {
  void loadAll();
});
</script>

<style scoped lang="scss">
.platform-security-policy {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.platform-security-policy__toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.security-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 16px;
  align-items: start;
}

.security-card {
  min-width: 0;

  :deep(.el-card__header) {
    padding: 20px 20px 0 !important;
    border-bottom: none !important;
  }

  :deep(.el-card__body) {
    padding: 12px 20px 20px !important;
  }
}

.card-header-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 15px;
  font-weight: 600;
}

.card-header-icon {
  font-size: 18px;
  color: var(--el-color-primary);
}

.settings-panel {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

:deep(.common-setting-row__label) {
  width: 140px;
  font-size: 13px;
  font-weight: 500;
}

:deep(.common-setting-row__control) {
  display: flex;
  justify-content: flex-end;
  overflow: visible;
  position: relative;
}

.setting-input-number,
.setting-select {
  width: 120px !important;
}

@media (max-width: 1100px) {
  .security-grid {
    grid-template-columns: 1fr;
  }
}
</style>
