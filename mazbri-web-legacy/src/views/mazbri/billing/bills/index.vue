<template>
  <ControlPanelPageShell
    :title="$t('menus.mazbri.billsAndReconciliation')"
    icon="ri:file-list-3-line"
  >
    <div class="bills-hub-page ops-page-stack">
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
        <div ref="tabActionsEl" class="page-header-tabs-actions" />
      </div>

      <div
        v-if="visited.has('overview')"
        v-show="activeTab === 'overview'"
        class="tab-panel"
      >
        <FinanceOverviewPanel :active="activeTab === 'overview'" />
      </div>
      <div
        v-if="visited.has('bills')"
        v-show="activeTab === 'bills'"
        class="tab-panel"
      >
        <BillListPanel :active="activeTab === 'bills'" />
      </div>
      <div
        v-if="visited.has('reconcile')"
        v-show="activeTab === 'reconcile'"
        class="tab-panel"
      >
        <UsageReconciliationPanel :active="activeTab === 'reconcile'" />
      </div>
    </div>
  </ControlPanelPageShell>
</template>

<script setup lang="ts">
import { computed, provide, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";
import { $t } from "@/locales";
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import UsageReconciliationPanel from "@/views/mazbri/billing/usage-reconciliation/index.vue";
import FinanceOverviewPanel from "./FinanceOverviewPanel.vue";
import BillListPanel from "./BillListPanel.vue";

defineOptions({ name: "BillList" });

type BillsHubTab = "overview" | "bills" | "reconcile";

const { locale } = useI18n();
const route = useRoute();
const router = useRouter();

const tabActionsEl = ref<HTMLElement | null>(null);
provide("billsTabActionsEl", tabActionsEl);
provide("resourceTabHeaderActionsEl", tabActionsEl);

const tabOptions = computed<Array<{ label: string; value: BillsHubTab }>>(
  () => {
    void locale.value;
    return [
      { label: $t("ops.finance.tabOverview"), value: "overview" },
      { label: $t("ops.finance.tabBills"), value: "bills" },
      { label: $t("ops.finance.tabReconcile"), value: "reconcile" },
    ];
  },
);

function resolveTab(): BillsHubTab {
  const raw = Array.isArray(route.query.tab)
    ? route.query.tab[0]
    : route.query.tab;
  if (raw === "bills" || raw === "overview" || raw === "reconcile") {
    return raw;
  }
  return "overview";
}

const activeTab = ref<BillsHubTab>(resolveTab());
const visited = reactive(new Set<BillsHubTab>([activeTab.value]));

watch(
  () => route.fullPath,
  () => {
    const next = resolveTab();
    visited.add(next);
    if (activeTab.value !== next) {
      activeTab.value = next;
    }
  },
);

async function handleTabChange(value: unknown) {
  let next: BillsHubTab = "overview";
  if (value === "bills" || value === "reconcile" || value === "overview") {
    next = value;
  }
  visited.add(next);
  activeTab.value = next;
  await router.replace({
    query: {
      ...route.query,
      tab: next,
    },
  });
}
</script>

<style scoped lang="scss">
.bills-hub-page {
  min-width: 0;
  display: flex;
  flex-direction: column;
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
  }
}
</style>
