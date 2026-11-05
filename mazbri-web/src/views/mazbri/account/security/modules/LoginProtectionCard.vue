<template>
  <SettingsCard
    v-loading="loading"
    icon="ri:login-box-line"
    :title="$t('securityPolicy.sections.loginProtection.title')"
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
    <CommonInlineHint block size="xs">
      {{ $t("securityPolicy.hints.lockThreshold") }}
    </CommonInlineHint>
    <CommonInlineHint block size="xs">
      <span>{{ $t("accountSecurity.platformPolicy.sessionHint") }}</span>
      <ElLink
        type="primary"
        underline="never"
        class="session-link"
        @click="goSystemSettings"
      >
        {{ $t("accountSecurity.platformPolicy.sessionLink") }}
      </ElLink>
    </CommonInlineHint>
  </SettingsCard>
</template>

<script setup lang="ts">
import { useRouter } from "vue-router";
import SettingsCard from "@/components/common/settings-card/index.vue";
import CommonSettingRow from "@/components/common/setting-row/index.vue";
import CommonInlineHint from "@/components/common/inline-hint/index.vue";
import { usePlatformSecurityPolicy } from "./usePlatformSecurityPolicy";

defineOptions({ name: "LoginProtectionCard" });

const router = useRouter();
const { policyForm, loading } = usePlatformSecurityPolicy();

function goSystemSettings(): void {
  void router.push({ path: "/control-panel/system" });
}
</script>

<style scoped lang="scss">
.session-link {
  margin-left: 6px;
  font-size: inherit;
  vertical-align: baseline;
}
</style>
