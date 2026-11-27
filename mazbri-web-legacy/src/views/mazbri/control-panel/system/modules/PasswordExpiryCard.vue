<template>
  <SettingsCard
    v-loading="loading"
    icon="ri:history-line"
    :title="$t('securityPolicy.sections.expiration.title')"
    :info="$t('securityPolicy.hints.expiration')"
  >
    <CommonSettingRow
      mode="inline"
      mobile-stack
      :label="$t('securityPolicy.fields.passwordValidity')"
    >
      <ElInputNumber
        v-model="policyForm.max_age_days"
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
        v-model="policyForm.warning_days"
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
        v-model="policyForm.prevent_reuse_count"
        :min="0"
        :max="24"
        class="setting-input-number"
      />
    </CommonSettingRow>
  </SettingsCard>
</template>

<script setup lang="ts">
import SettingsCard from "@/components/common/settings-card/index.vue";
import CommonSettingRow from "@/components/common/setting-row/index.vue";
import { usePlatformSecurityPolicy } from "./usePlatformSecurityPolicy";

defineOptions({ name: "PasswordExpiryCard" });

const { policyForm, loading } = usePlatformSecurityPolicy();
</script>
