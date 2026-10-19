<template>
  <div class="display-settings-body">
    <div class="settings-grid">
      <CommonToggleTile v-for="item in displaySettings" :key="item.key">
        <template #label>
          {{ item.label }}
        </template>
        <ElSwitch
          :model-value="getToggleValue(item.key)"
          size="small"
          @update:model-value="handleToggle(item.key, Boolean($event))"
        />
      </CommonToggleTile>
    </div>
  </div>
</template>

<script setup lang="ts">
import CommonToggleTile from "@/components/common/toggle-tile/index.vue";
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

<style scoped lang="scss">
.settings-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px 24px;
}

:deep(.common-toggle-tile) {
  background: transparent !important;
  border: none !important;
  padding: 6px 0 !important;
  min-height: auto !important;
}

@media (max-width: 768px) {
  .settings-grid {
    grid-template-columns: 1fr;
    gap: 4px 0;
  }
}
</style>
