<template>
  <OpsMergedShell
    :title="t('menus.mazbri.opsInfra')"
    :tabs="tabs"
    :default-tab="defaultTab"
  >
    <template #actions>
      <TimeRangePicker v-model="timeRange" />
      <AutoRefresh v-model="refreshInterval" @tick="bumpReload" />
    </template>

    <template #pipeline>
      <InfraMetricsPanel tab="pipeline" note-key="ops.infra.pipelineNote" />
    </template>
    <template #middleware>
      <InfraMetricsPanel
        tab="middleware"
        note-key="ops.infra.middlewareNote"
      />
    </template>
    <template #hosts>
      <InfraMetricsPanel tab="hosts" note-key="ops.infra.hostsNote" />
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
import InfraMetricsPanel from "./InfraMetricsPanel.vue";

defineOptions({ name: "OpsInfra" });

const LABEL_MAP = {
  pipeline: "menus.mazbri.infraPipeline",
  middleware: "menus.mazbri.infraMiddleware",
  hosts: "menus.mazbri.infraHosts",
} as const;

const { t } = useI18n();
const { tabs, defaultTab } = useMergedShellTabs("infra", LABEL_MAP);
const { timeRange, refreshInterval, bumpReload } = provideOpsSharedRange();
</script>
