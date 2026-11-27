<template>
  <OpsMergedShell
    :title="t('menus.mazbri.opsTraffic')"
    :tabs="tabs"
    :default-tab="defaultTab"
  >
    <template #actions>
      <TimeRangePicker v-model="timeRange" />
      <AutoRefresh v-model="refreshInterval" @tick="bumpReload" />
    </template>

    <template #customer>
      <TrafficCallsPanel mode="customer" />
    </template>
    <template #upstream>
      <TrafficCallsPanel mode="upstream" />
    </template>
    <template #traces>
      <TrafficTracesPanel />
    </template>
  </OpsMergedShell>
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import OpsMergedShell from "@/components/ops/OpsMergedShell.vue";
import TimeRangePicker from "@/components/ops/TimeRangePicker.vue";
import { useMergedShellTabs } from "@/composables/ops/useMergedShellTabs";
import { provideOpsSharedRange } from "@/composables/ops/useOpsSharedRange";
import TrafficCallsPanel from "./modules/TrafficCallsPanel.vue";
import TrafficTracesPanel from "./modules/TrafficTracesPanel.vue";

defineOptions({ name: "OpsTraffic" });

const LABEL_MAP = {
  customer: "ops.calls.tabs.customer",
  upstream: "ops.calls.tabs.upstream",
  traces: "ops.calls.tabs.traces",
} as const;

const { t } = useI18n();
const { tabs, defaultTab } = useMergedShellTabs("traffic", LABEL_MAP);
const { timeRange, refreshInterval, bumpReload } = provideOpsSharedRange(
  "1h",
  0,
);
</script>
