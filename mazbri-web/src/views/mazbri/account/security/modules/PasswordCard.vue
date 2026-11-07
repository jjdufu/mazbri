<template>
  <SettingsCard
    icon="ri:lock-password-line"
    :title="$t('accountSecurity.password.title')"
    :info="$t('accountSecurity.password.policyHint')"
  >
    <template #title-extra>
      <ElTag type="success" effect="light" size="small">
        {{ $t("accountSecurity.password.statusConfigured") }}
      </ElTag>
    </template>
    <template #actions>
      <template v-if="isEditing">
        <ElButton :disabled="passwordLoading" @click="cancelEdit">
          {{ $t("common.cancel") }}
        </ElButton>
        <ElButton
          type="primary"
          :loading="passwordLoading"
          @click="handleChangePassword"
        >
          {{ $t("common.confirm") }}
        </ElButton>
      </template>
      <ElButton v-else @click="startEdit">
        {{ $t("accountSecurity.actions.changePassword") }}
      </ElButton>
    </template>

    <template v-if="!isEditing">
      <CommonSettingRow
        mode="inline"
        mobile-stack
        :label="$t('accountSecurity.password.lengthLabel')"
      >
        <span class="setting-value">{{ lengthSummaryText }}</span>
      </CommonSettingRow>
      <CommonSettingRow
        mode="inline"
        mobile-stack
        :label="$t('accountSecurity.password.expiryLabel')"
      >
        <span class="setting-value">{{ expirySummaryText }}</span>
      </CommonSettingRow>
    </template>

    <template v-else>
      <CommonSettingRow
        mode="inline"
        mobile-stack
        :label="$t('accountSecurity.password.current')"
      >
        <ElInput
          v-model="passwordForm.current_password"
          type="password"
          show-password
          autocomplete="current-password"
          class="setting-input"
          @keyup.enter="handleChangePassword"
        />
      </CommonSettingRow>
      <CommonSettingRow
        mode="inline"
        mobile-stack
        :label="$t('accountSecurity.password.new')"
      >
        <ElInput
          v-model="passwordForm.new_password"
          type="password"
          show-password
          autocomplete="new-password"
          class="setting-input"
          @keyup.enter="handleChangePassword"
        />
      </CommonSettingRow>
      <CommonSettingRow
        mode="inline"
        mobile-stack
        :label="$t('accountSecurity.password.confirm')"
      >
        <ElInput
          v-model="passwordForm.confirm_password"
          type="password"
          show-password
          autocomplete="new-password"
          class="setting-input"
          @keyup.enter="handleChangePassword"
        />
      </CommonSettingRow>
    </template>
  </SettingsCard>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { ElMessage } from "element-plus";
import SettingsCard from "@/components/common/settings-card/index.vue";
import CommonSettingRow from "@/components/common/setting-row/index.vue";
import { changePassword } from "@/api/auth";
import { fetchPasswordPolicy } from "@/api/audit";
import type { ChangePasswordRequest } from "@/types/api";
import { runWithLoading } from "@/composables/useAsyncState";
import { useOpsRole } from "@/composables/ops/useOpsRole";
import { resolvePasswordPolicyPayload } from "@/utils/password";
import { $t } from "@/locales";
import { logger } from "@/utils/sys";
import { usePlatformSecurityPolicy } from "./usePlatformSecurityPolicy";

defineOptions({ name: "PasswordCard" });

const { isAdmin } = useOpsRole();
const { policyForm } = usePlatformSecurityPolicy();
const isEditing = ref(false);
const passwordLoading = ref(false);
const nonAdminPolicy = ref<{
  min_length: number;
  max_length: number;
  max_age_days: number;
  warning_days: number;
} | null>(null);

const passwordForm = reactive<ChangePasswordRequest>({
  current_password: "",
  new_password: "",
  confirm_password: "",
});

const lengthSummaryText = computed(() => {
  const min = isAdmin.value
    ? (policyForm.min_length ?? 8)
    : (nonAdminPolicy.value?.min_length ?? 8);
  const max = isAdmin.value
    ? (policyForm.max_length ?? 128)
    : (nonAdminPolicy.value?.max_length ?? 128);
  return $t("accountSecurity.password.summary.length", { min, max });
});

const expirySummaryText = computed(() => {
  const days = isAdmin.value
    ? (policyForm.max_age_days ?? 90)
    : (nonAdminPolicy.value?.max_age_days ?? 90);
  const warning = isAdmin.value
    ? (policyForm.warning_days ?? 7)
    : (nonAdminPolicy.value?.warning_days ?? 7);
  if (!days || days <= 0) {
    return $t("accountSecurity.password.summary.maxAgeOff");
  }
  return $t("accountSecurity.password.summary.maxAge", { days, warning });
});

function startEdit(): void {
  resetPasswordForm();
  isEditing.value = true;
}

function cancelEdit(): void {
  resetPasswordForm();
  isEditing.value = false;
}

function validatePasswordForm(): string | null {
  if (!passwordForm.current_password.trim()) {
    return $t("accountSecurity.password.currentRequired");
  }
  if (!passwordForm.new_password.trim()) {
    return $t("accountSecurity.password.newRequired");
  }
  if (!passwordForm.confirm_password.trim()) {
    return $t("accountSecurity.password.confirmRequired");
  }
  if (passwordForm.confirm_password !== passwordForm.new_password) {
    return $t("accountSecurity.password.mismatch");
  }
  return null;
}

function resetPasswordForm(): void {
  passwordForm.current_password = "";
  passwordForm.new_password = "";
  passwordForm.confirm_password = "";
}

async function handleChangePassword(): Promise<void> {
  const error = validatePasswordForm();
  if (error) {
    ElMessage.warning(error);
    return;
  }
  await runWithLoading(passwordLoading, async () => {
    await changePassword({ ...passwordForm });
    resetPasswordForm();
    isEditing.value = false;
  }).catch((err) => {
    logger.error("Failed to change password:", err);
  });
}

async function loadPasswordPolicySummary(): Promise<void> {
  if (isAdmin.value) return;
  try {
    const response = await fetchPasswordPolicy();
    const policy = resolvePasswordPolicyPayload(response);
    if (policy) {
      nonAdminPolicy.value = {
        min_length: policy.min_length,
        max_length: policy.max_length,
        max_age_days: policy.max_age_days,
        warning_days: policy.warning_days,
      };
    }
  } catch (err) {
    logger.warn("Failed to load password policy summary:", err);
  }
}

onMounted(() => {
  void loadPasswordPolicySummary();
});
</script>

<style scoped lang="scss">
.setting-value {
  font-size: 14px;
  color: var(--el-text-color-primary);
}
</style>
