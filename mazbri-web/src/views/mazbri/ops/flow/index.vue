<template>
  <OpsMergedShell
    :title="t('menus.mazbri.opsFlow')"
    :tabs="tabs"
    :default-tab="defaultTab"
  >
    <template #actions>
      <TimeRangePicker v-model="timeRange" />
      <AutoRefresh v-model="refreshInterval" @tick="bumpReload" />
    </template>

    <template #ratelimit>
      <RatelimitPanel />
    </template>
    <template #cache>
      <CachePanel />
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
import RatelimitPanel from "./RatelimitPanel.vue";
import CachePanel from "./CachePanel.vue";

defineOptions({ name: "OpsFlow" });

const LABEL_MAP = {
  ratelimit: "menus.mazbri.flowRatelimit",
  cache: "menus.mazbri.flowCache",
} as const;

const { t } = useI18n();
const { tabs, defaultTab } = useMergedShellTabs("flow", LABEL_MAP);
/** 产品默认时间窗 6h / 60s；缓存 Tab 共用。 */
const { timeRange, refreshInterval, bumpReload } = provideOpsSharedRange(
  "6h",
  60,
);
</script>
