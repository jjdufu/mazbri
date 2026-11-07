<template>
  <ControlPanelPageShell
    :title="$t('systemSettings.page.title')"
    icon="ri:settings-4-line"
  >
    <template #actions>
      <ElButton :disabled="saveLoading" @click="handleAppearanceReset">
        <template #icon><ArtSvgIcon icon="ri:restart-line" /></template>
        {{ $t("systemSettings.form.resetToDefault") }}
      </ElButton>
      <ElButton type="primary" :loading="saveLoading" @click="handleSaveAll">
        <template #icon><ArtSvgIcon icon="ri:save-line" /></template>
        {{ $t("systemSettings.form.saveSettings") }}
      </ElButton>
    </template>

    <SettingsCardGrid v-loading="pageLoading">
      <template #left>
        <SettingsCard
          icon="ri:settings-3-line"
          :title="$t('systemSettings.sections.basic')"
        >
          <BasicSettings v-model="systemForm" />
        </SettingsCard>
        <SettingsCard
          icon="ri:eye-line"
          :title="$t('systemSettings.display.title')"
        >
          <DisplaySettings
            :model-value="localAppearanceSettings"
            :display-settings="displaySettings"
            @update:model-value="handleAppearanceModelUpdate"
          />
        </SettingsCard>
      </template>

      <template #right>
        <SettingsCard
          icon="ri:palette-line"
          :title="$t('systemSettings.theme.title')"
        >
          <ThemeSettings
            :model-value="localAppearanceSettings"
            :main-colors="configOptions.mainColors"
            @update:model-value="handleAppearanceModelUpdate"
          />
        </SettingsCard>
        <SettingsCard
          icon="ri:layout-grid-line"
          :title="$t('systemSettings.layout.title')"
        >
          <LayoutSettings
            :model-value="localAppearanceSettings"
            :container-width-options="containerWidthOptions"
            :box-style-options="boxStyleOptionsWithIcon"
            @update:model-value="handleAppearanceModelUpdate"
          />
        </SettingsCard>
        <SettingsCard
          icon="ri:magic-line"
          :title="$t('systemSettings.style.title')"
        >
          <StyleSettings
            :model-value="localAppearanceSettings"
            :tab-style-options="tabStyleOptions"
            :page-transition-options="pageTransitionOptions"
            :custom-radius-options="customRadiusOptions"
            @update:model-value="handleAppearanceModelUpdate"
          />
        </SettingsCard>
      </template>
    </SettingsCardGrid>
  </ControlPanelPageShell>
</template>

<script setup lang="ts">
import { computed, onMounted, onActivated } from "vue";
import { ElMessage } from "element-plus";
import { useI18n } from "vue-i18n";
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import SettingsCardGrid from "@/components/common/settings-card-grid/index.vue";
import SettingsCard from "@/components/common/settings-card/index.vue";
import BasicSettings from "./modules/BasicSettings.vue";
import ThemeSettings from "./modules/ThemeSettings.vue";
import LayoutSettings from "./modules/LayoutSettings.vue";
import DisplaySettings from "./modules/DisplaySettings.vue";
import StyleSettings from "./modules/StyleSettings.vue";
import { useSystemSettings } from "./composables/useSystemSettings";
import { useAppearanceSettings } from "./composables/useAppearanceSettings";

defineOptions({ name: "SystemSettings" });

const { t } = useI18n();
const {
  loading,
  systemSaving,
  systemForm,
  loadSystemData,
  saveSystemSettings,
} = useSystemSettings();

const {
  configOptions,
  tabStyleOptions,
  pageTransitionOptions,
  customRadiusOptions,
  boxStyleOptionsWithIcon,
  containerWidthOptions,
  displaySettings,
  localAppearanceSettings,
  appearanceLoading,
  appearanceSaving,
  loadAppearanceSettings,
  saveAppearanceSettings,
  handleAppearanceReset,
} = useAppearanceSettings();

const handleAppearanceModelUpdate = (
  value: Partial<typeof localAppearanceSettings>,
) => {
  Object.assign(localAppearanceSettings, value);
};

const pageLoading = computed(() => loading.value || appearanceLoading.value);
const saveLoading = computed(
  () => systemSaving.value || appearanceSaving.value,
);

const handleSaveAll = async () => {
  const systemSaved = await saveSystemSettings({ silent: true });
  if (!systemSaved) {
    return;
  }
  const appearanceResult = await saveAppearanceSettings({ silent: true });
  if (appearanceResult !== "server") {
    return;
  }
  ElMessage.success(t("systemSettings.messages.savedSuccess"));
};

onMounted(() => {
  loadSystemData();
  loadAppearanceSettings();
});

onActivated(() => {
  loadSystemData();
  loadAppearanceSettings();
});
</script>
