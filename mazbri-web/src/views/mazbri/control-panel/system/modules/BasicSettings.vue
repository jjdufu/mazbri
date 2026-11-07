<template>
  <CommonSettingRow
    mode="inline"
    mobile-stack
    :label="$t('systemSettings.form.siteName')"
  >
    <ElInput
      :model-value="modelValue.site_name"
      :placeholder="$t('systemSettings.form.siteNamePlaceholder')"
      class="setting-input"
      @update:model-value="handleUpdate('site_name', $event)"
    />
  </CommonSettingRow>
  <CommonSettingRow
    mode="inline"
    mobile-stack
    :label="$t('systemSettings.form.siteUrl')"
  >
    <ElInput
      :model-value="modelValue.site_url"
      :placeholder="$t('systemSettings.form.siteUrlPlaceholder')"
      class="setting-input"
      @update:model-value="handleUpdate('site_url', $event)"
    />
  </CommonSettingRow>
  <CommonSettingRow
    mode="inline"
    mobile-stack
    :label="$t('systemSettings.form.tenantGatewayUrl')"
  >
    <ElInput
      :model-value="modelValue.tenant_gateway_url"
      :placeholder="$t('systemSettings.form.tenantGatewayUrlPlaceholder')"
      :title="$t('systemSettings.form.tenantGatewayUrlHint')"
      class="setting-input"
      @update:model-value="handleUpdate('tenant_gateway_url', $event)"
    />
  </CommonSettingRow>
  <CommonSettingRow
    mode="inline"
    mobile-stack
    :label="$t('systemSettings.form.adminEmail')"
  >
    <ElInput
      :model-value="modelValue.admin_email"
      :placeholder="$t('systemSettings.form.adminEmailPlaceholder')"
      class="setting-input"
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
</template>

<script setup lang="ts">
import CommonSettingRow from "@/components/common/setting-row/index.vue";

interface BasicSettingsModel {
  site_name?: string;
  site_url?: string;

  tenant_gateway_url?: string;
  admin_email?: string;
  session_timeout_minutes?: number;
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
