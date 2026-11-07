<template>
  <SettingsCard
    v-loading="loading"
    icon="ri:login-box-line"
    :title="$t('securityPolicy.sections.loginProtection.title')"
    :info="loginProtectionInfo"
  >
    <CommonSettingRow
      mode="inline"
      mobile-stack
      :label="$t('securityPolicy.fields.lockThreshold')"
    >
      <ElInputNumber
        v-model="policyForm.max_login_attempts"
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
        v-model="policyForm.lockout_duration_minutes"
        :min="5"
        :max="1440"
        class="setting-input-number"
      />
    </CommonSettingRow>
  </SettingsCard>
</template>

<script setup lang="ts">
import { computed } from "vue";
import SettingsCard from "@/components/common/settings-card/index.vue";
import CommonSettingRow from "@/components/common/setting-row/index.vue";
import { $t } from "@/locales";
import { usePlatformSecurityPolicy } from "./usePlatformSecurityPolicy";

defineOptions({ name: "LoginProtectionCard" });

const { policyForm, loading } = usePlatformSecurityPolicy();

const loginProtectionInfo = computed(
  () =>
    `${$t("securityPolicy.hints.lockThreshold")}\n${$t("accountSecurity.platformPolicy.sessionHint")}`,
);
</script>
