<template>
  <SettingsToggleGrid>
    <CommonToggleTile
      v-for="item in displaySettings"
      :key="item.key"
      :label="item.label"
    >
      <ElSwitch
        :model-value="getToggleValue(item.key)"
        size="small"
        @update:model-value="handleToggle(item.key, Boolean($event))"
      />
    </CommonToggleTile>
  </SettingsToggleGrid>
</template>

<script setup lang="ts">
import CommonToggleTile from "@/components/common/toggle-tile/index.vue";
import SettingsToggleGrid from "@/components/common/settings-toggle-grid/index.vue";
interface DisplaySettingItem {
  key: string;
  label: string;
}
interface DisplaySettingsModel {
  showWorkTab: boolean;
  showMenuButton: boolean;
  showRefreshButton: boolean;
  showCrumbs: boolean;
  showLanguage: boolean;
  showNprogress: boolean;
  watermarkVisible: boolean;
}
const props = defineProps<{
  modelValue: DisplaySettingsModel;
  displaySettings: DisplaySettingItem[];
}>();
const emit = defineEmits<{
  (e: "update:modelValue", value: DisplaySettingsModel): void;
}>();
const getToggleValue = (key: string): boolean => {
  return Boolean(
    (props.modelValue as unknown as Record<string, boolean | undefined>)[key],
  );
};
const handleToggle = (key: string, value: boolean) => {
  emit("update:modelValue", {
    ...props.modelValue,
    [key]: value,
  } as DisplaySettingsModel);
};
</script>
