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

    <div v-loading="pageLoading" class="strategy-grid">

      <div class="strategy-col">

        <ElCard shadow="hover" class="strategy-card">
          <template #header>
            <SurfaceCardHeader>
              <template #left>
                <div class="card-header-title">
                  <ArtSvgIcon
                    icon="ri:settings-3-line"
                    class="card-header-icon"
                  />
                  <span>{{ $t("systemSettings.sections.basic") }}</span>
                </div>
              </template>
            </SurfaceCardHeader>
          </template>
          <div class="settings-panel">
            <BasicSettings v-model="systemForm" />
          </div>
        </ElCard>

        <ElCard shadow="hover" class="strategy-card">
          <template #header>
            <SurfaceCardHeader>
              <template #left>
                <div class="card-header-title">
                  <ArtSvgIcon icon="ri:eye-line" class="card-header-icon" />
                  <span>{{ $t("systemSettings.display.title") }}</span>
                </div>
              </template>
            </SurfaceCardHeader>
          </template>
          <div class="settings-panel">
            <DisplaySettings
              :model-value="localAppearanceSettings"
              :display-settings="displaySettings"
              @update:model-value="handleAppearanceModelUpdate"
            />
          </div>
        </ElCard>
      </div>

      <div class="strategy-col">

        <ElCard shadow="hover" class="strategy-card">
          <template #header>
            <SurfaceCardHeader>
              <template #left>
                <div class="card-header-title">
                  <ArtSvgIcon icon="ri:palette-line" class="card-header-icon" />
                  <span>{{ $t("systemSettings.theme.title") }}</span>
                </div>
              </template>
            </SurfaceCardHeader>
          </template>
          <div class="settings-panel">
            <ThemeSettings
              :model-value="localAppearanceSettings"
              :main-colors="configOptions.mainColors"
              @update:model-value="handleAppearanceModelUpdate"
            />
          </div>
        </ElCard>

        <ElCard shadow="hover" class="strategy-card">
          <template #header>
            <SurfaceCardHeader>
              <template #left>
                <div class="card-header-title">
                  <ArtSvgIcon
                    icon="ri:layout-grid-line"
                    class="card-header-icon"
                  />
                  <span>{{ $t("systemSettings.layout.title") }}</span>
                </div>
              </template>
            </SurfaceCardHeader>
          </template>
          <div class="settings-panel">
            <LayoutSettings
              :model-value="localAppearanceSettings"
              :container-width-options="containerWidthOptions"
              :box-style-options="boxStyleOptionsWithIcon"
              @update:model-value="handleAppearanceModelUpdate"
            />
          </div>
        </ElCard>

        <ElCard shadow="hover" class="strategy-card">
          <template #header>
            <SurfaceCardHeader>
              <template #left>
                <div class="card-header-title">
                  <ArtSvgIcon icon="ri:magic-line" class="card-header-icon" />
                  <span>{{ $t("systemSettings.style.title") }}</span>
                </div>
              </template>
            </SurfaceCardHeader>
          </template>
          <div class="settings-panel">
            <StyleSettings
              :model-value="localAppearanceSettings"
              :tab-style-options="tabStyleOptions"
              :page-transition-options="pageTransitionOptions"
              :custom-radius-options="customRadiusOptions"
              @update:model-value="handleAppearanceModelUpdate"
            />
          </div>
        </ElCard>
      </div>
    </div>
  </ControlPanelPageShell>
</template>

<script setup lang="ts">
import { computed, onMounted, onActivated } from "vue";
import { ElMessage } from "element-plus";
import { useI18n } from "vue-i18n";
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import SurfaceCardHeader from "@/components/common/surface-card-header/index.vue";
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
  await saveSystemSettings({ silent: true });
  const appearanceResult = await saveAppearanceSettings({ silent: true });
  if (appearanceResult === "local") {
    ElMessage.warning(t("systemSettings.messages.appearanceSavedLocally"));
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

<style scoped lang="scss">

.strategy-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 16px;
  align-items: stretch;
}

.strategy-col {
  display: flex;
  flex-direction: column;
  gap: 16px;
  height: 100%;
}

.strategy-card {
  min-width: 0;
  display: flex;
  flex-direction: column;
  overflow: visible;

  &:last-child {
    flex: 1;
  }

  :deep(.el-card__header) {
    padding: 20px 20px 0px !important;
    border-bottom: none !important;
  }

  :deep(.el-card__body) {
    padding: 0 20px 20px !important;
    overflow: visible !important;
  }
}

.card-header-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-weight: 600;
  font-size: 15px;
}

.card-header-icon {
  font-size: 18px;
  color: var(--el-color-primary);
}

.settings-panel {
  background: var(--el-fill-color-lighter);
  border: 1px solid var(--el-border-color-light);
  border-radius: var(--custom-radius);
  padding: 16px 20px;
  margin-top: 10px;

  overflow: visible;
}

@media (max-width: 1100px) {
  .strategy-grid {
    grid-template-columns: 1fr;
  }
}
</style>
