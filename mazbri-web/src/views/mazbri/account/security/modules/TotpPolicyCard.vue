<template>
  <SettingsCard
    :id="PLATFORM_POLICY_ANCHOR"
    v-loading="loading"
    icon="ri:shield-keyhole-line"
    :title="$t('securityPolicy.sections.totp.title')"
  >
    <CommonInlineHint block size="xs">
      {{ $t("accountSecurity.platformPolicy.description") }}
    </CommonInlineHint>
    <MfaRequiredHint block />
    <CommonSettingRow
      mode="inline"
      mobile-stack
      :label="$t('securityPolicy.fields.issuerName')"
    >
      <ExpandOnFocusInput
        v-model="mfaForm.totp_issuer"
        :placeholder="$t('securityPolicy.placeholders.issuerName')"
        :compact-width="120"
      />
    </CommonSettingRow>
    <CommonSettingRow
      mode="inline"
      mobile-stack
      :label="$t('securityPolicy.fields.totpDigits')"
    >
      <ElSelect v-model="mfaForm.totp_digits" class="setting-select">
        <ElOption :label="$t('securityPolicy.options.totpDigits6')" :value="6" />
        <ElOption :label="$t('securityPolicy.options.totpDigits8')" :value="8" />
      </ElSelect>
    </CommonSettingRow>
    <CommonSettingRow
      mode="inline"
      mobile-stack
      :label="$t('securityPolicy.fields.totpPeriod')"
    >
      <ElSelect v-model="mfaForm.totp_period" class="setting-select">
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
        class="setting-input-number"
      />
    </CommonSettingRow>
  </SettingsCard>
</template>

<script setup lang="ts">
import SettingsCard from "@/components/common/settings-card/index.vue";
import CommonSettingRow from "@/components/common/setting-row/index.vue";
import CommonInlineHint from "@/components/common/inline-hint/index.vue";
import ExpandOnFocusInput from "@/components/common/expand-on-focus-input/index.vue";
import MfaRequiredHint from "@/components/common/mfa-required-hint/index.vue";
import {
  PLATFORM_POLICY_ANCHOR,
  usePlatformSecurityPolicy,
} from "./usePlatformSecurityPolicy";

defineOptions({ name: "TotpPolicyCard" });

const { mfaForm, loading } = usePlatformSecurityPolicy();
</script>
