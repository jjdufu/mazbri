<template>
  <ControlPanelPageShell
    :title="$t('menus.mazbri.upstreamSupply')"
    icon="ri:building-4-line"
  >
    <div class="upstream-supply-page ops-page-stack">
      <div class="page-header-tabs-bar">
        <ElTabs
          :model-value="activeTab"
          class="page-header-tabs"
          @tab-change="handleTabChange"
        >
          <ElTabPane
            v-for="item in tabOptions"
            :key="item.value"
            :label="item.label"
            :name="item.value"
          />
        </ElTabs>
        <div ref="headerActionsEl" class="page-header-tabs-actions" />
      </div>

      <div v-show="activeTab === 'suppliers'" class="tab-panel">
        <SupplierList
          v-if="loadedTabs.suppliers"
          ref="supplierListRef"
          @access="openMicroserviceAccess"
        />
      </div>
      <div v-show="activeTab === 'channels'" class="tab-panel">
        <UpstreamChannelList
          v-if="loadedTabs.channels"
          ref="channelListRef"
          :active-tab="activeTab"
        />
      </div>
      <div v-show="activeTab === 'drafts'" class="tab-panel">
        <ConnectorDraftList v-if="loadedTabs.drafts" />
      </div>
    </div>

    <UpstreamAccessWizard v-model="accessVisible" @success="onAccessSuccess" />
  </ControlPanelPageShell>
</template>

<script setup lang="ts">
import { computed, provide, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { $t } from "@/locales";
import { useTabRouteSync } from "@/composables/useTabRouteSync";
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import UpstreamChannelList from "@/views/mazbri/channels/index.vue";
import SupplierList from "@/views/mazbri/suppliers/index.vue";
import ConnectorDraftList from "@/views/mazbri/upstream-connectors/drafts.vue";
import UpstreamAccessWizard from "@/views/mazbri/upstreams/UpstreamAccessWizard.vue";

defineOptions({ name: "UpstreamSupply" });

type UpstreamSupplyTab = "suppliers" | "channels" | "drafts";

const { locale } = useI18n();
const headerActionsEl = ref<HTMLElement | null>(null);
provide("resourceTabHeaderActionsEl", headerActionsEl);

const supplierListRef = ref<InstanceType<typeof SupplierList> | null>(null);
const channelListRef = ref<InstanceType<typeof UpstreamChannelList> | null>(
  null,
);
const accessVisible = ref(false);

const { activeTab, handleTabChange } = useTabRouteSync<UpstreamSupplyTab>({
  defaultValue: "suppliers",
  allowedValues: ["suppliers", "channels", "drafts"],
});

const loadedTabs = reactive<Record<UpstreamSupplyTab, boolean>>({
  suppliers: false,
  channels: false,
  drafts: false,
});

watch(
  activeTab,
  (tab) => {
    loadedTabs[tab] = true;
  },
  { immediate: true },
);

const tabOptions = computed(() => {
  void locale.value;
  return [
    { label: $t("menus.mazbri.supplierList"), value: "suppliers" },
    { label: $t("menus.mazbri.upstreamChannelList"), value: "channels" },
    { label: $t("upstreamConnector.draftsTitle"), value: "drafts" },
  ];
});

function openMicroserviceAccess() {
  accessVisible.value = true;
}

function onAccessSuccess() {
  void supplierListRef.value?.loadData();
  if (loadedTabs.channels) {
    void channelListRef.value?.loadData();
    void channelListRef.value?.checkHasActiveSupplier();
  }
}
</script>

<style scoped lang="scss">
.upstream-supply-page {
  min-width: 0;
  display: flex;
  flex-direction: column;
  flex: 1;
  height: 100%;
  gap: var(--ops-section-gap, 16px);

  .page-header-tabs-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px 16px;
    flex-shrink: 0;
  }

  .page-header-tabs-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-left: auto;
  }

  .page-header-tabs {
    margin-bottom: 0;
    flex-shrink: 0;

    :deep(.el-tabs__header) {
      margin: 0 !important;
    }

    :deep(.el-tabs__content) {
      display: none !important;
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
