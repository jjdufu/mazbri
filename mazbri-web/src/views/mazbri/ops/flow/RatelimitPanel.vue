<template>
  <OpsMetricChartGrid
    :charts="charts"
    :series="series"
    :loading="loading"
  />
</template>

<script setup lang="ts">
import { computed } from "vue";
import OpsMetricChartGrid from "@/components/ops/OpsMetricChartGrid.vue";
import { useMetricTimeseriesPanel } from "@/composables/ops/useMetricTimeseriesPanel";
import { useOpsSharedRange } from "@/composables/ops/useOpsSharedRange";
import { getFlowCharts } from "@/utils/ops/flow-charts";

defineOptions({ name: "OpsFlowRatelimitPanel" });

const { timeRange, reloadTick } = useOpsSharedRange();
const charts = computed(() => getFlowCharts("ratelimit"));
const { loading, series } = useMetricTimeseriesPanel({
  charts,
  timeRange,
  reloadTick,
});
</script>
