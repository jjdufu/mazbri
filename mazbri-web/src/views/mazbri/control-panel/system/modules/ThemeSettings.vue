<template>
  <div class="theme-settings-body">
    <CommonSettingRow
      mode="inline"
      mobile-stack
      :label="$t('systemSettings.theme.themeMode')"
    >
      <div class="theme-mode-options">
        <button
          v-for="option in themeModeOptions"
          :key="option.value"
          type="button"
          class="theme-mode-option"
          :class="{ 'is-active': modelValue.systemThemeMode === option.value }"
          @click="handleThemeModeSelect(option.value)"
        >
          {{ option.label }}
        </button>
      </div>
    </CommonSettingRow>
    <CommonSettingRow
      mode="inline"
      mobile-stack
      :label="$t('systemSettings.theme.systemThemeColor')"
    >
      <div class="color-picker-wrapper">
        <div
          v-for="color in mainColors"
          :key="color"
          class="color-item"
          :class="{ 'is-active': modelValue.systemThemeColor === color }"
          :style="{ background: color }"
          @click="handleColorSelect(color)"
        >
          <ArtSvgIcon
            v-show="modelValue.systemThemeColor === color"
            icon="ri:check-fill"
            class="check-icon"
          />
        </div>
      </div>
    </CommonSettingRow>
  </div>
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { computed } from "vue";
import { SystemThemeEnum } from "@/enums/appEnum";
import CommonSettingRow from "@/components/common/setting-row/index.vue";
import CommonInlineHint from "@/components/common/inline-hint/index.vue";
const { t } = useI18n();
interface ThemeSettingsModel {
  systemThemeMode: SystemThemeEnum;
  systemThemeColor: string;
  darkThemeStyle: "obsidian" | "graphite" | "oceanic" | "nordic";
}
const props = defineProps<{
  modelValue: ThemeSettingsModel;
  mainColors: readonly string[];
}>();
const emit = defineEmits<{
  (e: "update:modelValue", value: ThemeSettingsModel): void;
}>();
const handleColorSelect = (color: string) => {
  emit("update:modelValue", { ...props.modelValue, systemThemeColor: color });
};
const themeModeOptions = computed(() => [
  { value: SystemThemeEnum.LIGHT, label: t("setting.theme.list[0]") },
  { value: SystemThemeEnum.DARK, label: t("setting.theme.list[1]") },
  { value: SystemThemeEnum.AUTO, label: t("setting.theme.list[2]") },
]);
const handleThemeModeSelect = (mode: SystemThemeEnum) => {
  emit("update:modelValue", { ...props.modelValue, systemThemeMode: mode });
};
</script>

<style scoped lang="scss">
.theme-settings-body {
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
  flex-direction: column;
  align-items: flex-end;
  justify-content: flex-end;
  width: 100%;
}

.theme-mode-options {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  margin-bottom: 8px;
  justify-content: flex-end;
}

.theme-mode-option {
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

.color-picker-wrapper {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  justify-content: flex-end;

  .color-item {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.2s;
    border: 2px solid transparent;
    &:hover {
      transform: scale(1.1);
    }
    &.is-active {
      border-color: var(--el-color-primary);
      box-shadow: 0 0 0 2px rgba(22, 119, 255, 0.2);
    }
    .check-icon {
      color: white;
      font-size: 16px;
    }
  }
}
</style>
