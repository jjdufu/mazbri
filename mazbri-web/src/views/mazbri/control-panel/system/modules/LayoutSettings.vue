<template>
  <div class="layout-settings-body">
    <CommonSettingRow
      mode="inline"
      mobile-stack
      :label="$t('systemSettings.layout.containerWidth')"
    >
      <div class="layout-options">
        <button
          v-for="option in containerWidthOptions"
          :key="option.value"
          type="button"
          class="layout-option"
          :class="{ 'is-active': modelValue.containerWidth === option.value }"
          @click="handleContainerWidthChange(option.value)"
        >
          {{ option.label }}
        </button>
      </div>
    </CommonSettingRow>
    <CommonSettingRow
      mode="inline"
      mobile-stack
      :label="$t('systemSettings.layout.boxStyle')"
    >
      <div class="layout-options">
        <button
          v-for="option in boxStyleOptions"
          :key="option.type"
          type="button"
          class="layout-option"
          :class="{
            'is-active':
              modelValue.boxBorderMode === (option.type === 'border-mode'),
          }"
          @click="handleBoxStyleChange(option.type === 'border-mode')"
        >
          {{ option.label }}
        </button>
      </div>
    </CommonSettingRow>
  </div>
</template>

<script setup lang="ts">
import CommonSettingRow from "@/components/common/setting-row/index.vue";

interface ContainerWidthOption {
  value: "100%" | "1200px";
  label: string;
  icon: string;
}

interface BoxStyleOption {
  type: string;
  label: string;
}

interface LayoutSettingsModel {
  containerWidth: "100%" | "1200px";
  boxBorderMode: boolean;
}

const props = defineProps<{
  modelValue: LayoutSettingsModel;
  containerWidthOptions: ContainerWidthOption[];
  boxStyleOptions: BoxStyleOption[];
}>();

const emit = defineEmits<{
  (e: "update:modelValue", value: LayoutSettingsModel): void;
}>();

const handleContainerWidthChange = (value: "100%" | "1200px") => {
  emit("update:modelValue", { ...props.modelValue, containerWidth: value });
};

const handleBoxStyleChange = (boxBorderMode: boolean) => {
  emit("update:modelValue", { ...props.modelValue, boxBorderMode });
};
</script>

<style scoped lang="scss">
.layout-settings-body {
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

.layout-options {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  justify-content: flex-end;
}

.layout-option {
  min-width: 72px;
  height: 34px;
  padding: 0 14px;
  border: 1px solid var(--el-border-color);
  border-radius: var(--art-form-control-radius);
  background: var(--default-box-color);
  color: var(--el-text-color-regular);
  cursor: pointer;
  transition:
    border-color var(--el-transition-duration),
    background-color var(--el-transition-duration),
    color var(--el-transition-duration);

  &:hover {
    border-color: var(--el-color-primary-light-5);
    color: var(--el-text-color-primary);
  }

  &.is-active {
    border-color: var(--el-color-primary);
    background: color-mix(
      in srgb,
      var(--el-color-primary) 8%,
      var(--default-box-color)
    );
    color: var(--el-color-primary);
  }
}
</style>
