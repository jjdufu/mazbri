<template>
  <ControlPanelPageShell
    :title="$t('securityPolicy.page.title')"
    icon="ri:lock-password-line"
  >
    <template #actions>
      <ElButton
        v-if="canEdit"
        type="primary"
        :loading="saving"
        @click="handleSaveAll"
      >
        <template #icon><ArtSvgIcon icon="ri:save-line" /></template>
        {{ $t("securityPolicy.actions.saveSettings") }}
      </ElButton>
    </template>

    <div v-if="canEdit" v-loading="loading" class="strategy-grid">

      <div class="strategy-col">

        <ElCard shadow="hover" class="strategy-card">
          <template #header>
            <SurfaceCardHeader>
              <template #left>
                <div class="card-header-title">
                  <ArtSvgIcon
                    icon="ri:shield-keyhole-line"
                    class="card-header-icon"
                  />
                  <span>{{
                    $t("securityPolicy.sections.complexity.title")
                  }}</span>
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
                class="setting-input-number"
              />
            </CommonSettingRow>
            <CommonSettingRow
              mode="inline"
              mobile-stack
              :label="$t('securityPolicy.fields.requireUppercase')"
            >
              <ElSwitch v-model="editForm.require_uppercase" />
            </CommonSettingRow>
            <CommonSettingRow
              mode="inline"
              mobile-stack
              :label="$t('securityPolicy.fields.requireLowercase')"
            >
              <ElSwitch v-model="editForm.require_lowercase" />
            </CommonSettingRow>
            <CommonSettingRow
              mode="inline"
              mobile-stack
              :label="$t('securityPolicy.fields.requireDigit')"
            >
              <ElSwitch v-model="editForm.require_digit" />
            </CommonSettingRow>
            <CommonSettingRow
              mode="inline"
              mobile-stack
              :label="$t('securityPolicy.fields.requireSpecialChar')"
            >
              <ElSwitch v-model="editForm.require_special_char" />
            </CommonSettingRow>
          </div>
        </ElCard>

        <ElCard shadow="hover" class="strategy-card">
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
                class="setting-input-number"
              />
            </CommonSettingRow>
          </div>
        </ElCard>

        <ElCard shadow="hover" class="strategy-card">
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
                class="setting-input-number"
              />
            </CommonSettingRow>
          </div>
        </ElCard>
      </div>

      <div class="strategy-col">

        <ElCard shadow="hover" class="strategy-card">
          <template #header>
            <SurfaceCardHeader>
              <template #left>
                <div class="card-header-title">
                  <ArtSvgIcon
                    icon="ri:smartphone-line"
                    class="card-header-icon"
                  />
                  <span>{{
                    $t("securityPolicy.sections.mfaScope.title")
                  }}</span>
                </div>
              </template>
            </SurfaceCardHeader>
          </template>
          <div class="settings-panel">
            <CommonSettingRow
              mode="inline"
              mobile-stack
              class="mfa-scope-row"
              :label="$t('securityPolicy.fields.globalMfa')"
            >
              <ElRadioGroup v-model="mfaMode" class="mfa-scope-radios">
                <ElRadio value="disabled">{{
                  $t("securityPolicy.options.disabled")
                }}</ElRadio>
                <ElRadio value="admin_only">{{
                  $t("securityPolicy.options.adminOnly")
                }}</ElRadio>
                <ElRadio value="all_users">{{
                  $t("securityPolicy.options.allUsers")
                }}</ElRadio>
              </ElRadioGroup>
            </CommonSettingRow>
          </div>
        </ElCard>

        <ElCard shadow="hover" class="strategy-card">
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
          <div
            class="settings-panel"
            :class="{ 'is-disabled': mfaConfigDisabled }"
          >
            <CommonSettingRow
              mode="inline"
              mobile-stack
              :label="$t('securityPolicy.fields.issuerName')"
            >
              <ExpandOnFocusInput
                v-model="mfaForm.totp_issuer"
                :placeholder="$t('securityPolicy.placeholders.issuerName')"
                :disabled="mfaConfigDisabled"
                :compact-width="120"
              />
            </CommonSettingRow>
            <CommonSettingRow
              mode="inline"
              mobile-stack
              :label="$t('securityPolicy.fields.totpDigits')"
            >
              <ElSelect
                v-model="mfaForm.totp_digits"
                :disabled="mfaConfigDisabled"
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
                :disabled="mfaConfigDisabled"
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
                :disabled="mfaConfigDisabled"
                class="setting-input-number"
              />
            </CommonSettingRow>
          </div>
        </ElCard>

        <ElCard shadow="hover" class="strategy-card">
          <template #header>
            <SurfaceCardHeader>
              <template #left>
                <div class="card-header-title">
                  <ArtSvgIcon
                    icon="ri:question-line"
                    class="card-header-icon"
                  />
                  <span>{{ $t("securityPolicy.sections.process.title") }}</span>
                </div>
              </template>
            </SurfaceCardHeader>
          </template>
          <div
            class="settings-panel"
            :class="{ 'is-disabled': mfaConfigDisabled }"
          >
            <div class="mfa-instructions">
              <ElSteps :active="3" direction="vertical" class="mfa-steps">
                <ElStep :title="$t('securityPolicy.steps.install.title')">
                  <template #description>
                    {{ $t("securityPolicy.steps.install.description") }}
                  </template>
                </ElStep>
                <ElStep :title="$t('securityPolicy.steps.scan.title')">
                  <template #description>
                    {{ $t("securityPolicy.steps.scan.description") }}
                  </template>
                </ElStep>
                <ElStep :title="$t('securityPolicy.steps.enter.title')">
                  <template #description>
                    {{ $t("securityPolicy.steps.enter.description") }}
                  </template>
                </ElStep>
              </ElSteps>
            </div>
          </div>
        </ElCard>
      </div>
    </div>

    <ElCard v-else shadow="hover" class="strategy-card">
      <ElAlert type="warning" :closable="false">
        {{ $t("securityPolicy.messages.noPermission") }}
      </ElAlert>
    </ElCard>
  </ControlPanelPageShell>
</template>

<script setup lang="ts">
function unwrapData<T>(response: T | { data?: T }): T {
  if (response && typeof response === "object" && "data" in response && (response as { data?: T }).data !== undefined) {
    return (response as { data: T }).data;
  }
  return response as T;
}
import { ref, reactive, computed, onMounted, onActivated } from "vue";
import { ElMessage } from "element-plus";
import { toRaw } from "vue";
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import SurfaceCardHeader from "@/components/common/surface-card-header/index.vue";
import CommonSettingRow from "@/components/common/setting-row/index.vue";
import ExpandOnFocusInput from "@/components/common/expand-on-focus-input/index.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { useUserStore } from "@/store/modules/user";
import {
  fetchPasswordPolicy,
  updatePasswordPolicy,
  fetchMfaSettings,
  updateMfaSettings,
  type UpdatePasswordPolicyRequest,
  type UpdateMfaSettingsRequest,
  type MfaSettings,
} from "@/api/audit";
import { $t } from "@/locales";
import { logger } from "@/utils/sys";

defineOptions({ name: "PasswordPolicy" });

interface PasswordPolicyData {
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
  updated_at?: string;
}

const userStore = useUserStore();
const canEdit = computed(() => userStore.isAdmin);
const loading = ref(false);
const saving = ref(false);
const policy = ref<PasswordPolicyData | null>(null);

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
  mfa_enabled_globally: false,
  mfa_required_for_all: false,
  mfa_required_for_roles: ["R_ADMIN"],
  totp_issuer: "Mazbri",
  totp_digits: 6,
  totp_period: 30,
  backup_codes_count: 10,
});

const mfaMode = computed({
  get() {
    if (!mfaForm.mfa_enabled_globally) return "disabled";
    if (mfaForm.mfa_required_for_all) return "all_users";
    return "admin_only";
  },
  set(val: string) {
    if (val === "disabled") {
      mfaForm.mfa_enabled_globally = false;
      mfaForm.mfa_required_for_all = false;
      mfaForm.mfa_required_for_roles = [];
    } else if (val === "admin_only") {
      mfaForm.mfa_enabled_globally = true;
      mfaForm.mfa_required_for_all = false;
      mfaForm.mfa_required_for_roles = ["R_ADMIN"];
    } else {
      mfaForm.mfa_enabled_globally = true;
      mfaForm.mfa_required_for_all = true;
      mfaForm.mfa_required_for_roles = [
        "R_ADMIN",
        "R_FINANCE",
        "R_VIEWER",
      ];
    }
  },
});

const mfaConfigDisabled = computed(() => mfaMode.value === "disabled");

const loadData = async () => {
  await runWithLoading(loading, async () => {
    const response = await fetchPasswordPolicy();
    policy.value = unwrapData(response);
    if (policy.value) {
      Object.assign(editForm, {
        min_length: policy.value.min_length,
        max_length: policy.value.max_length,
        require_uppercase: policy.value.require_uppercase,
        require_lowercase: policy.value.require_lowercase,
        require_digit: policy.value.require_digit,
        require_special_char: policy.value.require_special_char,
        special_chars: policy.value.special_chars,
        max_age_days: policy.value.max_age_days,
        warning_days: policy.value.warning_days,
        prevent_reuse_count: policy.value.prevent_reuse_count,
        max_login_attempts: policy.value.max_login_attempts,
        lockout_duration_minutes: policy.value.lockout_duration_minutes,
      });
    }
  }).catch((error) => {
    logger.error("Failed to load password policy:", error);
  });
};

const loadMfaData = async () => {
  try {
    const res = await fetchMfaSettings();
    const data = res.data;
    mfaForm.mfa_enabled_globally = data.mfa_enabled_globally;
    mfaForm.mfa_required_for_all = data.mfa_required_for_all;
    mfaForm.mfa_required_for_roles = data.mfa_required_for_roles || ["R_ADMIN"];
    mfaForm.totp_issuer = data.totp_issuer || "Mazbri";
    mfaForm.totp_digits = data.totp_digits || 6;
    mfaForm.totp_period = data.totp_period || 30;
    mfaForm.backup_codes_count = data.backup_codes_count || 10;
  } catch (error) {
    logger.error("Failed to load MFA settings:", error);
  }
};

const saveMfa = async () => {
  const payload: UpdateMfaSettingsRequest = {
    mfa_enabled_globally: mfaForm.mfa_enabled_globally,
    mfa_required_for_all: mfaForm.mfa_required_for_all,
    mfa_required_for_roles: mfaForm.mfa_required_for_roles,
    totp_issuer: mfaForm.totp_issuer,
    totp_digits: mfaForm.totp_digits,
    totp_period: mfaForm.totp_period,
    backup_codes_count: mfaForm.backup_codes_count,
  };
  await updateMfaSettings(payload);
};

const savePolicy = async () => {
  const response = await updatePasswordPolicy(toRaw(editForm));
  policy.value = unwrapData(response);
};

const handleSaveAll = async () => {
  await runWithLoading(saving, async () => {
    await Promise.all([savePolicy(), saveMfa()]);
    ElMessage.success($t("securityPolicy.messages.savedSuccess"));
  }).catch((error) => {
    logger.error("Failed to save settings:", error);
  });
};

onMounted(() => {
  loadData();
  loadMfaData();
});

onActivated(() => {
  loadData();
  loadMfaData();
});
</script>

<style scoped lang="scss">

.strategy-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 16px;
  align-items: stretch;
}

.strategy-col {
  display: flex;
  flex-direction: column;
  gap: 16px;
  height: 100%;
}

.strategy-card {
  min-width: 0;
  display: flex;
  flex-direction: column;

  &:last-child {
    flex: 1;
  }
}

.strategy-card {
  :deep(.el-card__header) {
    padding: 20px 20px 0px !important;
    border-bottom: none !important;
  }

  :deep(.el-card__body) {
    padding: 0 20px 20px !important;
  }
}

.card-header-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-weight: 600;
  font-size: 15px;
}

.card-header-icon {
  font-size: 18px;
  color: var(--el-color-primary);
}

.settings-panel {
  // Flat section inside the card: no nested bordered/gray box.
  background: transparent;
  border: none;
  border-radius: 0;
  padding: 4px 0 0;
  margin-top: 8px;
  display: flex;
  flex-direction: column;
  gap: 14px;

  &.is-disabled {
    opacity: 0.55;
    pointer-events: none;
    filter: grayscale(0.2);
  }
}

:deep(.common-setting-row--inline) {
  gap: 12px;
}

:deep(.common-setting-row__label) {
  width: 140px;
  font-size: 13px;
  font-weight: 500;
}

:deep(.common-setting-row__control) {
  display: flex;
  justify-content: flex-end;
}


:deep(.mfa-scope-row.common-setting-row--inline) {
  align-items: flex-start;
}

:deep(.mfa-scope-row .common-setting-row__control) {
  justify-content: flex-start;
  flex: 1;
  min-width: 0;
}

.mfa-scope-radios {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 10px;
  width: 100%;

  :deep(.el-radio) {
    margin-right: 0;
    height: auto;
    white-space: nowrap;
  }
}


.setting-input-number,
.setting-select {
  width: 120px !important;
}

:deep(.common-setting-row__control) {
  overflow: visible;
  position: relative;
}

.setting-radio-group {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  justify-content: flex-end;
}

.mfa-instructions {
  padding-top: 4px;
}

.mfa-steps {
  :deep(.el-step__description) {
    font-size: 13px;
    line-height: 1.7;
  }
}

@media (max-width: 1100px) {
  .strategy-grid {
    grid-template-columns: 1fr;
  }
}
</style>
