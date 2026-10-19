<template>
  <div class="style-settings-body">
    <CommonSettingRow
      mode="inline"
      mobile-stack
      :label="$t('systemSettings.style.menuWidth')"
    >
      <ElInputNumber
        :model-value="modelValue.menuOpenWidth"
        :min="180"
        :max="320"
        :step="10"
        class="setting-input-number"
        @update:model-value="handleUpdate('menuOpenWidth', $event)"
      />
    </CommonSettingRow>
    <CommonSettingRow
      mode="inline"
      mobile-stack
      :label="$t('systemSettings.style.tabStyle')"
    >
      <ElSelect
        :model-value="modelValue.tabStyle"
        class="setting-select"
        @update:model-value="handleUpdate('tabStyle', $event)"
      >
        <ElOption
          v-for="option in tabStyleOptions"
          :key="option.value"
          :label="option.label"
          :value="option.value"
        />
      </ElSelect>
    </CommonSettingRow>
    <CommonSettingRow
      mode="inline"
      mobile-stack
      :label="$t('systemSettings.style.pageTransition')"
    >
      <ElSelect
        :model-value="modelValue.pageTransition"
        class="setting-select"
        @update:model-value="handleUpdate('pageTransition', $event)"
      >
        <ElOption
          v-for="option in pageTransitionOptions"
          :key="option.value"
          :label="option.label"
          :value="option.value"
        />
      </ElSelect>
    </CommonSettingRow>
    <CommonSettingRow
      mode="inline"
      mobile-stack
      :label="$t('systemSettings.style.customRadius')"
    >
      <ElSelect
        :model-value="modelValue.customRadius"
        class="setting-select"
        @update:model-value="handleUpdate('customRadius', $event)"
      >
        <ElOption
          v-for="option in customRadiusOptions"
          :key="option.value"
          :label="option.label"
          :value="option.value"
        />
      </ElSelect>
    </CommonSettingRow>
  </div>
</template>

<script setup lang="ts">
import CommonSettingRow from "@/components/common/setting-row/index.vue";
interface SelectOption {
  label: string;
  value: string;
}
interface StyleSettingsModel {
  menuOpenWidth: number;
  tabStyle: string;
  pageTransition: string;
  customRadius: string;
}
const props = defineProps<{
  modelValue: StyleSettingsModel;
  tabStyleOptions: SelectOption[];
  pageTransitionOptions: SelectOption[];
  customRadiusOptions: SelectOption[];
}>();
const emit = defineEmits<{
  (e: "update:modelValue", value: StyleSettingsModel): void;
}>();
const handleUpdate = <K extends keyof StyleSettingsModel>(
  key: K,
  value: StyleSettingsModel[K] | undefined,
) => {
  if (value === undefined) {
    return;
  }
  emit("update:modelValue", { ...props.modelValue, [key]: value });
};
</script>

<style scoped lang="scss">
.style-settings-body {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding-top: 4px;
}

:deep(.common-setting-row--inline) {
  gap: 12px;
}

:deep(.common-setting-row__label) {
  width: 110px;
  font-size: 13px;
  font-weight: 500;
}

:deep(.common-setting-row__control) {
  display: flex;
  justify-content: flex-end;
}


.setting-input-number,
.setting-select {
  width: 120px !important;
}
</style>
