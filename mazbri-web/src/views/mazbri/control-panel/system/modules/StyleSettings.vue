<template>
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
