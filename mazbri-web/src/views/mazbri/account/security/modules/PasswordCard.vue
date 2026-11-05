<template>
  <SettingsCard
    icon="ri:lock-password-line"
    :title="$t('accountSecurity.password.title')"
  >
    <template #actions>
      <ElButton
        type="primary"
        :loading="passwordLoading"
        @click="handleChangePassword"
      >
        {{ $t("accountSecurity.actions.changePassword") }}
      </ElButton>
    </template>

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
        class="password-input"
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
        class="password-input"
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
        class="password-input"
        @keyup.enter="handleChangePassword"
      />
    </CommonSettingRow>

    <!-- 非超管只读展示当前密码策略摘要；超管在右列策略卡片里直接编辑 -->
    <CommonSettingRow
      v-if="!isAdmin"
      mode="block"
      :label="$t('accountSecurity.password.summaryTitle')"
    >
      <ul v-if="passwordPolicyLines.length" class="policy-summary">
        <li v-for="(line, idx) in passwordPolicyLines" :key="idx">
          {{ line }}
        </li>
      </ul>
      <CommonInlineHint v-else block size="xs">
        {{ $t("accountSecurity.password.policyHint") }}
      </CommonInlineHint>
    </CommonSettingRow>
  </SettingsCard>
</template>

<script setup lang="ts">
import { onMounted, reactive, ref } from "vue";
import { ElMessage } from "element-plus";
import SettingsCard from "@/components/common/settings-card/index.vue";
import CommonSettingRow from "@/components/common/setting-row/index.vue";
import CommonInlineHint from "@/components/common/inline-hint/index.vue";
import { changePassword } from "@/api/auth";
import { fetchPasswordPolicy } from "@/api/audit";
import type { ChangePasswordRequest } from "@/types/api";
import { runWithLoading } from "@/composables/useAsyncState";
import { useOpsRole } from "@/composables/ops/useOpsRole";
import {
  formatPasswordPolicySummaryLines,
  resolvePasswordPolicyPayload,
} from "@/utils/password";
import { $t } from "@/locales";
import { logger } from "@/utils/sys";

defineOptions({ name: "PasswordCard" });

const { isAdmin } = useOpsRole();
const passwordLoading = ref(false);
const passwordPolicyLines = ref<string[]>([]);

const passwordForm = reactive<ChangePasswordRequest>({
  current_password: "",
  new_password: "",
  confirm_password: "",
});

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
  }).catch((err) => {
    logger.error("Failed to change password:", err);
  });
}

async function loadPasswordPolicySummary(): Promise<void> {
  if (isAdmin.value) {
    passwordPolicyLines.value = [];
    return;
  }
  try {
    const response = await fetchPasswordPolicy();
    const policy = resolvePasswordPolicyPayload(response);
    passwordPolicyLines.value = policy
      ? formatPasswordPolicySummaryLines(policy, $t)
      : [];
  } catch (err) {
    passwordPolicyLines.value = [];
    logger.warn("Failed to load password policy summary:", err);
  }
}

onMounted(() => {
  void loadPasswordPolicySummary();
});
</script>

<style scoped lang="scss">
.password-input {
  width: min(100%, 320px);
}

.policy-summary {
  margin: 0;
  padding-left: 18px;
  list-style: disc;
  font-size: 13px;
  line-height: 1.7;
  color: var(--el-text-color-secondary);
}

@media (max-width: 640px) {
  .password-input {
    width: 100%;
  }
}
</style>
