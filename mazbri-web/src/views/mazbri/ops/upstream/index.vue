<template>
  <OpsMergedShell
    :title="t('menus.mazbri.opsUpstream')"
    :tabs="tabs"
    :default-tab="defaultTab"
  >
    <template #actions>
      <TimeRangePicker v-model="timeRange" />
      <AutoRefresh v-model="refreshInterval" @tick="bumpReload" />
    </template>

    <template #sla>
      <SlaPanel />
    </template>
    <template #breaker>
      <BreakerPanel />
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
import SlaPanel from "./SlaPanel.vue";
import BreakerPanel from "./BreakerPanel.vue";

defineOptions({ name: "OpsUpstream" });

const LABEL_MAP = {
  sla: "menus.mazbri.upstreamSla",
  breaker: "menus.mazbri.upstreamBreaker",
} as const;

const { t } = useI18n();
const { tabs, defaultTab } = useMergedShellTabs("upstream", LABEL_MAP);
/** 与原 SLA 叶页一致：默认 24h / 60s；熔断 Tab 共用。 */
const { timeRange, refreshInterval, bumpReload } = provideOpsSharedRange(
  "24h",
  60,
);
</script>
