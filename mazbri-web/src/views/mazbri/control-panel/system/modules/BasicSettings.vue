<template>
  <div class="settings-form-body">
    <div class="basic-settings-grid">
      <CommonSettingRow
        mode="inline"
        mobile-stack
        :label="$t('systemSettings.form.siteName')"
      >
        <ExpandOnFocusInput
          :model-value="modelValue.site_name"
          :placeholder="$t('systemSettings.form.siteNamePlaceholder')"
          :compact-width="120"
          @update:model-value="handleUpdate('site_name', $event)"
        />
      </CommonSettingRow>
      <CommonSettingRow
        mode="inline"
        mobile-stack
        :label="$t('systemSettings.form.siteUrl')"
      >
        <ExpandOnFocusInput
          :model-value="modelValue.site_url"
          :placeholder="$t('systemSettings.form.siteUrlPlaceholder')"
          :compact-width="120"
          @update:model-value="handleUpdate('site_url', $event)"
        />
      </CommonSettingRow>
      <CommonSettingRow
        mode="inline"
        mobile-stack
        :label="$t('systemSettings.form.tenantGatewayUrl')"
      >
        <ExpandOnFocusInput
          :model-value="modelValue.tenant_gateway_url"
          :placeholder="$t('systemSettings.form.tenantGatewayUrlPlaceholder')"
          :title="$t('systemSettings.form.tenantGatewayUrlHint')"
          :compact-width="120"
          @update:model-value="handleUpdate('tenant_gateway_url', $event)"
        />
      </CommonSettingRow>
      <CommonSettingRow
        mode="inline"
        mobile-stack
        :label="$t('systemSettings.form.adminEmail')"
      >
        <ExpandOnFocusInput
          :model-value="modelValue.admin_email"
          :placeholder="$t('systemSettings.form.adminEmailPlaceholder')"
          :compact-width="120"
          @update:model-value="handleUpdate('admin_email', $event)"
        />
      </CommonSettingRow>
      <CommonSettingRow
        mode="inline"
        mobile-stack
        :label="$t('systemSettings.form.sessionTimeout')"
      >
        <ElInputNumber
          :model-value="modelValue.session_timeout_minutes"
          :min="1"
          :max="1440"
          class="setting-input-number"
          @update:model-value="handleUpdate('session_timeout_minutes', $event)"
        />
      </CommonSettingRow>
      <CommonSettingRow
        mode="inline"
        mobile-stack
        :label="$t('systemSettings.form.maxLoginAttempts')"
      >
        <ElInputNumber
          :model-value="modelValue.max_login_attempts"
          :min="1"
          :max="10"
          class="setting-input-number"
          @update:model-value="handleUpdate('max_login_attempts', $event)"
        />
      </CommonSettingRow>
    </div>
  </div>
</template>

<script setup lang="ts">
import CommonSettingRow from "@/components/common/setting-row/index.vue";
import ExpandOnFocusInput from "@/components/common/expand-on-focus-input/index.vue";

interface BasicSettingsModel {
  site_name?: string;
  site_url?: string;

  tenant_gateway_url?: string;
  admin_email?: string;
  session_timeout_minutes?: number;
  max_login_attempts?: number;
}

const props = defineProps<{
  modelValue: BasicSettingsModel;
}>();

const emit = defineEmits<{
  (e: "update:modelValue", value: BasicSettingsModel): void;
}>();

const handleUpdate = <K extends keyof BasicSettingsModel>(
  key: K,
  value: BasicSettingsModel[K] | undefined | string | number | null,
) => {
  if (value === undefined || value === null) return;
  emit("update:modelValue", {
    ...props.modelValue,
    [key]: value as BasicSettingsModel[K],
  });
};
</script>

<style scoped lang="scss">
.basic-settings-grid {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.setting-input-number {
  width: 120px !important;
}

:deep(.common-setting-row__label) {
  width: 140px;
  font-weight: 500;
  white-space: nowrap;
}

:deep(.common-setting-row__control) {
  display: flex;
  justify-content: flex-end;
  align-items: center;
  overflow: visible;
  position: relative;
}

:deep(.common-setting-row--inline) {
  overflow: visible;
}
</style>
