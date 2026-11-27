<template>
  <SettingsCard
    v-loading="loading"
    icon="ri:smartphone-line"
    :title="$t('securityPolicy.sections.totp.globalTitle')"
    :info="$t('securityPolicy.hints.totpIssuer')"
  >
    <CommonSettingRow
      mode="inline"
      mobile-stack
      :label="$t('securityPolicy.fields.issuerName')"
    >
      <ElInput
        v-model="mfaForm.totp_issuer"
        class="setting-input"
        :disabled="saving"
        :placeholder="$t('securityPolicy.placeholders.issuerName')"
      />
    </CommonSettingRow>
    <CommonSettingRow
      mode="inline"
      mobile-stack
      :label="$t('accountSecurity.mfa.specLabel')"
    >
      <span class="spec-summary">
        TOTP {{ mfaForm.totp_digits }} {{ $t("securityPolicy.units.digits") }} ·
        {{ mfaForm.totp_period }}s · {{ mfaForm.backup_codes_count }}
        {{ $t("securityPolicy.units.backupCodes") }}
      </span>
    </CommonSettingRow>
  </SettingsCard>
</template>

<script setup lang="ts">
import SettingsCard from "@/components/common/settings-card/index.vue";
import CommonSettingRow from "@/components/common/setting-row/index.vue";
import { usePlatformSecurityPolicy } from "./usePlatformSecurityPolicy";

defineOptions({ name: "GlobalMfaCard" });

const { mfaForm, loading, saving } = usePlatformSecurityPolicy();
</script>

<style scoped lang="scss">
.issuer-inline {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  max-width: 320px;
}

.spec-summary {
  font-size: 13px;
  color: var(--el-text-color-secondary);
}
</style>
