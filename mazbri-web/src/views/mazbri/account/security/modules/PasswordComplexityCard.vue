<template>
  <SettingsCard
    v-loading="loading"
    icon="ri:shield-check-line"
    :title="$t('securityPolicy.sections.complexity.title')"
  >
    <CommonSettingRow
      mode="inline"
      mobile-stack
      :label="$t('securityPolicy.fields.minLength')"
    >
      <ElInputNumber
        v-model="policyForm.min_length"
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
        v-model="policyForm.max_length"
        :min="32"
        :max="256"
        class="setting-input-number"
      />
    </CommonSettingRow>
    <SettingsToggleGrid>
      <CommonToggleTile
        v-for="item in toggles"
        :key="item.key"
        :label="$t(item.label)"
      >
        <ElSwitch v-model="policyForm[item.key]" size="small" />
      </CommonToggleTile>
    </SettingsToggleGrid>
    <CommonSettingRow
      mode="inline"
      mobile-stack
      :label="$t('securityPolicy.fields.specialChars')"
    >
      <ExpandOnFocusInput
        v-model="policyForm.special_chars"
        :placeholder="$t('securityPolicy.placeholders.specialChars')"
        :compact-width="160"
        :expand-width="320"
        :disabled="!policyForm.require_special_char"
      />
    </CommonSettingRow>
    <CommonInlineHint v-if="policyForm.require_special_char" block size="xs">
      {{ $t("securityPolicy.hints.specialChars") }}
    </CommonInlineHint>
  </SettingsCard>
</template>

<script setup lang="ts">
import SettingsCard from "@/components/common/settings-card/index.vue";
import SettingsToggleGrid from "@/components/common/settings-toggle-grid/index.vue";
import CommonSettingRow from "@/components/common/setting-row/index.vue";
import CommonToggleTile from "@/components/common/toggle-tile/index.vue";
import CommonInlineHint from "@/components/common/inline-hint/index.vue";
import ExpandOnFocusInput from "@/components/common/expand-on-focus-input/index.vue";
import { usePlatformSecurityPolicy } from "./usePlatformSecurityPolicy";

defineOptions({ name: "PasswordComplexityCard" });

type ToggleKey =
  | "require_uppercase"
  | "require_lowercase"
  | "require_digit"
  | "require_special_char";

const toggles: ReadonlyArray<{ key: ToggleKey; label: string }> = [
  { key: "require_uppercase", label: "securityPolicy.fields.requireUppercase" },
  { key: "require_lowercase", label: "securityPolicy.fields.requireLowercase" },
  { key: "require_digit", label: "securityPolicy.fields.requireDigit" },
  {
    key: "require_special_char",
    label: "securityPolicy.fields.requireSpecialChar",
  },
];

const { policyForm, loading } = usePlatformSecurityPolicy();
</script>
