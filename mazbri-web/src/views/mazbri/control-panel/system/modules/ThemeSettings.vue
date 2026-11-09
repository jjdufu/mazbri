<template>
  <CommonSettingRow
    mode="inline"
    mobile-stack
    :label="$t('systemSettings.theme.themeMode')"
  >
    <ElSegmented
      :model-value="modelValue.systemThemeMode"
      :options="themeModeOptions"
      @change="handleThemeModeSelect"
    />
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
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { computed } from "vue";
import { SystemThemeEnum } from "@/enums/appEnum";
import CommonSettingRow from "@/components/common/setting-row/index.vue";
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
.color-picker-wrapper {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  justify-content: flex-end;

  .color-item {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.2s;
    border: 2px solid transparent;
    &:hover {
      transform: scale(1.08);
    }
    &.is-active {
      border-color: var(--el-color-primary);
      box-shadow: 0 0 0 2px rgba(22, 119, 255, 0.2);
    }
    .check-icon {
      color: white;
      font-size: 15px;
    }
  }
}
</style>
