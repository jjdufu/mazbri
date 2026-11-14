<template>
  <ControlPanelPageShell
    :title="$t('systemSettings.page.title')"
    icon="ri:settings-4-line"
  >
    <ElTabs
      :model-value="activeTab"
      class="platform-settings-tabs"
      @tab-change="handleTabChange"
    >
      <ElTabPane
        name="basic"
        :label="$t('systemSettings.tabs.basic')"
        lazy
      >
        <BasicSettings
          :model-value="systemForm"
          :loading="loading"
          :saving="systemSaving"
          @update:model-value="handleSystemFormUpdate"
          @save="handleSystemSave"
        />
      </ElTabPane>

      <ElTabPane
        name="security"
        :label="$t('systemSettings.tabs.security')"
        lazy
      >
        <SecurityPolicyTab />
      </ElTabPane>

      <ElTabPane
        name="license"
        :label="$t('systemSettings.tabs.license')"
        lazy
      >
        <PlatformLicenseTab />
      </ElTabPane>
    </ElTabs>
  </ControlPanelPageShell>
</template>

<script setup lang="ts">
import { onMounted, watch } from "vue";
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import { useTabRouteSync } from "@/composables/useTabRouteSync";
import BasicSettings from "./modules/BasicSettings.vue";
import SecurityPolicyTab from "./modules/SecurityPolicyTab.vue";
import PlatformLicenseTab from "./modules/PlatformLicenseTab.vue";
import {
  useSystemSettings,
  type BasicSystemFormState,
} from "./composables/useSystemSettings";

defineOptions({ name: "PlatformSettingsPage" });

type PlatformTab = "basic" | "security" | "license";

const { activeTab, handleTabChange } = useTabRouteSync<PlatformTab>({
  defaultValue: "basic",
  allowedValues: ["basic", "security", "license"],
});

const {
  loading,
  systemSaving,
  systemForm,
  loadSystemData,
  handleSystemSave,
} = useSystemSettings();

let basicLoaded = false;

const ensureBasicLoaded = () => {
  if (basicLoaded || activeTab.value !== "basic") return;
  basicLoaded = true;
  void loadSystemData();
};

const handleSystemFormUpdate = (nextValue: BasicSystemFormState) => {
  Object.assign(systemForm, nextValue);
};

onMounted(() => {
  ensureBasicLoaded();
});

watch(activeTab, () => {
  ensureBasicLoaded();
});
</script>

<style scoped lang="scss">
.platform-settings-tabs {
  :deep(.el-tabs__header) {
    margin-bottom: 14px;
  }
}
</style>
