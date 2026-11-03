<template>
  <OpsMergedShell
    :title="t('menus.mazbri.opsServices')"
    :tabs="tabs"
    :default-tab="defaultTab"
  >
    <template #actions>
      <AutoRefresh v-model="refreshInterval" @tick="bumpReload" />
    </template>

    <template #health>
      <HealthPanel />
    </template>
    <template #topology>
      <TopologyPanel />
    </template>
  </OpsMergedShell>
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import OpsMergedShell from "@/components/ops/OpsMergedShell.vue";
import { useMergedShellTabs } from "@/composables/ops/useMergedShellTabs";
import { provideOpsSharedRange } from "@/composables/ops/useOpsSharedRange";
import HealthPanel from "./HealthPanel.vue";
import TopologyPanel from "./TopologyPanel.vue";

defineOptions({ name: "OpsServices" });

const LABEL_MAP = {
  health: "menus.mazbri.serviceHealth",
  topology: "menus.mazbri.serviceTopology",
} as const;

const { t } = useI18n();
const { tabs, defaultTab } = useMergedShellTabs("services", LABEL_MAP);
/** 与原拓扑叶页一致：默认 1h / 30s；健康矩阵共用刷新。 */
const { refreshInterval, bumpReload } = provideOpsSharedRange("1h", 30);
</script>
