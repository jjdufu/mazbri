<template>
  <ControlPanelPageShell
    :title="$t('systemSettings.page.title')"
    icon="ri:settings-4-line"
  >
    <ElTabs
      :model-value="activeTab"
      :before-leave="handleBeforeTabLeave"
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
          :dirty="isBasicDirty"
          @update:model-value="handleSystemFormUpdate"
          @save="handleSystemSave"
        />
      </ElTabPane>

      <ElTabPane
        name="security"
        :label="$t('systemSettings.tabs.security')"
        lazy
      >
        <SecurityPolicyTab ref="securityTabRef" />
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
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import { onBeforeRouteLeave } from "vue-router";
import { ElMessageBox } from "element-plus";
import { $t } from "@/locales";
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
  isBasicDirty,
  loadSystemData,
  handleSystemSave,
} = useSystemSettings();

const securityTabRef = ref<{ isDirty?: boolean } | null>(null);

const isCurrentTabDirty = (): boolean => {
  if (activeTab.value === "basic") return isBasicDirty.value;
  if (activeTab.value === "security") return Boolean(securityTabRef.value?.isDirty);
  return false;
};

const confirmDiscardDirty = async (): Promise<boolean> => {
  if (!isCurrentTabDirty()) return true;
  try {
    await ElMessageBox.confirm(
      $t("systemSettings.unsavedLeaveConfirm"),
      $t("common.tips"),
      { type: "warning" },
    );
    return true;
  } catch {
    return false;
  }
};

const handleBeforeTabLeave = async (
  nextName: string | number,
  oldName: string | number,
): Promise<boolean> => {
  if (nextName === oldName) return true;
  return confirmDiscardDirty();
};

const onBeforeUnload = (event: BeforeUnloadEvent) => {
  if (!isCurrentTabDirty()) return;
  event.preventDefault();
  event.returnValue = "";
};

onBeforeRouteLeave(async () => confirmDiscardDirty());

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
  window.addEventListener("beforeunload", onBeforeUnload);
});

onBeforeUnmount(() => {
  window.removeEventListener("beforeunload", onBeforeUnload);
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
