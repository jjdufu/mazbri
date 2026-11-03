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

    <template #red>
      <RedPanel />
    </template>
    <template #latency>
      <LatencyPanel />
    </template>
    <template #errors>
      <ErrorsPanel />
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
import RedPanel from "./RedPanel.vue";
import LatencyPanel from "./LatencyPanel.vue";
import ErrorsPanel from "./ErrorsPanel.vue";

defineOptions({ name: "OpsTraffic" });

const LABEL_MAP = {
  red: "menus.mazbri.trafficRed",
  latency: "menus.mazbri.trafficLatency",
  errors: "menus.mazbri.trafficErrors",
} as const;

const { t } = useI18n();
const { tabs, defaultTab } = useMergedShellTabs("traffic", LABEL_MAP);
/** 与原叶页一致：默认 6h / 30s 刷新；子面板共用。 */
const { timeRange, refreshInterval, bumpReload } = provideOpsSharedRange(
  "6h",
  30,
);
</script>
