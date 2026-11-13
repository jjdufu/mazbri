<template>
  <OpsMergedShell
    :title="t('menus.mazbri.opsRuntime')"
    :tabs="tabs"
    :default-tab="defaultTab"
  >
    <template #actions>
      <TimeRangePicker v-model="timeRange" />
      <AutoRefresh v-model="refreshInterval" @tick="bumpReload" />
    </template>

    <template #apps>
      <HealthPanel />
    </template>
    <template #middleware>
      <InfraMetricsPanel
        tab="middleware"
        note-key="ops.infra.middlewareNote"
      />
    </template>
    <template #pipeline>
      <div class="ops-runtime__pipeline-stack">
        <BillingPipelinePanel v-if="isBiz" />
        <InfraMetricsPanel
          v-if="isAdmin"
          tab="pipeline"
          note-key="ops.infra.pipelineNote"
        />
      </div>
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
import { useOpsRole } from "@/composables/ops/useOpsRole";
import HealthPanel from "@/views/mazbri/ops/services/HealthPanel.vue";
import InfraMetricsPanel from "@/views/mazbri/ops/infra/InfraMetricsPanel.vue";
import BillingPipelinePanel from "./modules/BillingPipelinePanel.vue";

defineOptions({ name: "OpsRuntime" });

const LABEL_MAP = {
  apps: "menus.mazbri.runtimeApps",
  middleware: "menus.mazbri.infraMiddleware",
  pipeline: "menus.mazbri.infraPipeline",
  hosts: "menus.mazbri.infraHosts",
} as const;

const { t } = useI18n();
const { isAdmin, isBiz } = useOpsRole();
const { tabs, defaultTab } = useMergedShellTabs("runtime", LABEL_MAP);
const { timeRange, refreshInterval, bumpReload } = provideOpsSharedRange(
  "1h",
  30,
);
</script>

<style scoped>
.ops-runtime__pipeline-stack {
  display: flex;
  flex-direction: column;
  gap: var(--ops-section-gap, 16px);
  min-width: 0;
}
</style>
