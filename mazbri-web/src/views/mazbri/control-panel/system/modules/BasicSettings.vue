<template>
  <SettingsCardGrid v-loading="loading">
    <template #left>
      <SettingsCard
        icon="ri:global-line"
        :title="$t('systemSettings.sections.basic')"
        :info="$t('systemSettings.sections.basicInfo')"
      >
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
      </SettingsCard>
    </template>

    <template #right>
      <SettingsCard
        icon="ri:shield-star-line"
        :title="$t('systemSettings.sections.runtimeSecurity')"
        :info="$t('systemSettings.sections.runtimeSecurityInfo')"
      >
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
            @update:model-value="
              handleUpdate('session_timeout_minutes', $event)
            "
          />
        </CommonSettingRow>
        <CommonSettingRow
          mode="inline"
          mobile-stack
          :label="$t('systemSettings.form.maxUploadSize')"
        >
          <ElInputNumber
            :model-value="modelValue.max_upload_size_mb"
            :min="1"
            :max="512"
            class="setting-input-number"
            @update:model-value="handleUpdate('max_upload_size_mb', $event)"
          />
        </CommonSettingRow>
        <SettingsToggleGrid>
          <CommonToggleTile :label="$t('systemSettings.form.enableAuditLog')">
            <ElSwitch
              :model-value="modelValue.enable_audit_log"
              @update:model-value="
                handleUpdate('enable_audit_log', Boolean($event))
              "
            />
          </CommonToggleTile>
          <CommonToggleTile :label="$t('systemSettings.form.watermarkVisible')">
            <ElSwitch
              :model-value="modelValue.watermark_visible"
              @update:model-value="
                handleUpdate('watermark_visible', Boolean($event))
              "
            />
          </CommonToggleTile>
        </SettingsToggleGrid>
      </SettingsCard>
    </template>
  </SettingsCardGrid>

  <div class="tab-save-bar">
    <ElButton type="primary" :loading="saving" @click="emit('save')">
      <template #icon><ArtSvgIcon icon="ri:save-line" /></template>
      {{ $t("systemSettings.form.saveBasicSettings") }}
    </ElButton>
  </div>
</template>

<script setup lang="ts">
import SettingsCardGrid from "@/components/common/settings-card-grid/index.vue";
import SettingsCard from "@/components/common/settings-card/index.vue";
import SettingsToggleGrid from "@/components/common/settings-toggle-grid/index.vue";
import CommonSettingRow from "@/components/common/setting-row/index.vue";
import CommonToggleTile from "@/components/common/toggle-tile/index.vue";
import type { BasicSystemFormState } from "../composables/useSystemSettings";

defineOptions({ name: "BasicSettings" });

const props = withDefaults(
  defineProps<{
    modelValue: BasicSystemFormState;
    loading?: boolean;
    saving?: boolean;
  }>(),
  {
    loading: false,
    saving: false,
  },
);

const emit = defineEmits<{
  (e: "update:modelValue", value: BasicSystemFormState): void;
  (e: "save"): void;
}>();

const handleUpdate = <K extends keyof BasicSystemFormState>(
  key: K,
  value: BasicSystemFormState[K] | undefined | null,
) => {
  if (value === undefined || value === null) return;
  emit("update:modelValue", {
    ...props.modelValue,
    [key]: value,
  });
};
</script>

<style scoped lang="scss">
.tab-save-bar {
  display: flex;
  justify-content: flex-end;
  padding-top: 4px;
}
</style>
