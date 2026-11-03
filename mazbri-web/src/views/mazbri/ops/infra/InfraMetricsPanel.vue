<template>
  <OpsMetricChartGrid
    :charts="charts"
    :series="series"
    :loading="loading"
    :note="noteText"
  />
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import OpsMetricChartGrid from "@/components/ops/OpsMetricChartGrid.vue";
import { useMetricTimeseriesPanel } from "@/composables/ops/useMetricTimeseriesPanel";
import { useOpsSharedRange } from "@/composables/ops/useOpsSharedRange";
import {
  getInfraCharts,
  type InfraChartTab,
} from "@/utils/ops/infra-charts";

defineOptions({ name: "InfraMetricsPanel" });

const props = defineProps<{
  tab: InfraChartTab;
  noteKey: string;
}>();

const { t } = useI18n();
const { timeRange, reloadTick } = useOpsSharedRange();

const charts = computed(() => getInfraCharts(props.tab));
const noteText = computed(() => t(props.noteKey));

const { loading, series } = useMetricTimeseriesPanel({
  charts,
  timeRange,
  reloadTick,
});
</script>
