<template>
  <ControlPanelPageShell
    :title="$t('menus.mazbri.upstreamSupply')"
    icon="ri:building-4-line"
  >
    <div class="upstream-supply-page">
      <div class="tab-segmented">
        <ElSegmented
          :model-value="activeTab"
          :options="tabOptions"
          @change="handleTabChange"
        />
      </div>

      <div v-show="activeTab === 'suppliers'" class="tab-panel">
        <SupplierList ref="supplierListRef" @access="openMicroserviceAccess" />
      </div>
      <div v-show="activeTab === 'channels'" class="tab-panel">
        <UpstreamChannelList :active-tab="activeTab" />
      </div>
      <div v-show="activeTab === 'drafts'" class="tab-panel">
        <ConnectorDraftList />
      </div>
    </div>

    <UpstreamAccessWizard v-model="accessVisible" @success="onAccessSuccess" />
  </ControlPanelPageShell>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { $t } from "@/locales";
import { useTabRouteSync } from "@/composables/useTabRouteSync";
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import UpstreamChannelList from "@/views/mazbri/channels/index.vue";
import SupplierList from "@/views/mazbri/suppliers/index.vue";
import ConnectorDraftList from "@/views/mazbri/upstream-connectors/drafts.vue";
import UpstreamAccessWizard from "@/views/mazbri/upstreams/UpstreamAccessWizard.vue";

defineOptions({ name: "UpstreamSupply" });

type UpstreamSupplyTab = "suppliers" | "channels" | "drafts";

const supplierListRef = ref<InstanceType<typeof SupplierList> | null>(null);
const accessVisible = ref(false);

const { activeTab, handleTabChange } = useTabRouteSync<UpstreamSupplyTab>({
  defaultValue: "suppliers",
  allowedValues: ["suppliers", "channels", "drafts"],
});

const tabOptions = [
  { label: $t("menus.mazbri.supplierList"), value: "suppliers" },
  { label: $t("menus.mazbri.upstreamChannelList"), value: "channels" },
  { label: $t("upstreamConnector.draftsTitle"), value: "drafts" },
];

function openMicroserviceAccess() {
  accessVisible.value = true;
}

function onAccessSuccess() {
  void (supplierListRef.value as { loadData?: () => void } | null)?.loadData?.();
}
</script>

<style scoped lang="scss">
.upstream-supply-page {
  min-width: 0;
  display: flex;
  flex-direction: column;
  flex: 1;
  height: 100%;

  .tab-segmented {
    margin-bottom: 16px;
    flex-shrink: 0;

    .el-segmented {
      height: 40px;
      padding: 4px;
    }
  }

  .tab-panel {
    min-width: 0;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }
}

.upstream-supply-page :deep(.supplier-page),
.upstream-supply-page :deep(.channel-page) {
  min-width: 0;
  flex: 1;
  min-height: 0;
}
</style>
