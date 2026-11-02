<template>
  <OpsPage :title="t('menus.mazbri.infraHosts')">
    <template #actions>
      <TimeRangePicker v-model="timeRange" @change="reload" />
        <AutoRefresh v-model="refreshInterval" @tick="reload" />
    </template>
    <p class="ops-infra__note">{{ t("ops.infra.hostsNote") }}</p>
    <ElRow :gutter="16">
      <ElCol v-for="chart in chartDefs" :key="chart.metric" :xs="24" :lg="12">
        <OpsCard :title="t(chart.title)">
          <MultiSeriesChart
            :series="series[chart.metric]"
            :loading="loading"
            height="220px"
          />
        </OpsCard>
      </ElCol>
    </ElRow>
  </OpsPage>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import TimeRangePicker, {
  type TimeRangeValue,
} from "@/components/ops/TimeRangePicker.vue";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import MultiSeriesChart from "@/components/ops/MultiSeriesChart.vue";
import OpsPage from "@/components/ops/OpsPage.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import { useStep } from "@/composables/ops/useStep";
import { fetchMetricTimeseries } from "@/api/monitoring";
import type { MetricSeries } from "@/types/api/monitoring";
import { withContainerDisplayName } from "@/utils/ops/container-name";

defineOptions({ name: "OpsInfraHosts" });

const CHART_METRICS = [
  "host_cpu_usage_ratio", "host_load1", "host_memory_usage_ratio", "host_filesystem_usage_ratio", "host_disk_read_bytes", "host_disk_write_bytes", "host_network_rx_bytes", "host_network_tx_bytes", "container_cpu_cores", "container_memory_mb", "container_restarts", "container_oom_events", "container_network_rx_bytes", "container_network_tx_bytes",
] as const;
type ChartMetric = (typeof CHART_METRICS)[number];

const chartDefs = [
  { metric: "host_cpu_usage_ratio" as const, title: "ops.infra.charts.hostCpu" },
  { metric: "host_load1" as const, title: "ops.infra.charts.hostLoad" },
  { metric: "host_memory_usage_ratio" as const, title: "ops.infra.charts.hostMem" },
  { metric: "host_filesystem_usage_ratio" as const, title: "ops.infra.charts.hostFs" },
  { metric: "host_disk_read_bytes" as const, title: "ops.infra.charts.hostDiskRead" },
  { metric: "host_disk_write_bytes" as const, title: "ops.infra.charts.hostDiskWrite" },
  { metric: "host_network_rx_bytes" as const, title: "ops.infra.charts.hostNetRx" },
  { metric: "host_network_tx_bytes" as const, title: "ops.infra.charts.hostNetTx" },
  { metric: "container_cpu_cores" as const, title: "ops.infra.charts.ctrCpu" },
  { metric: "container_memory_mb" as const, title: "ops.infra.charts.ctrMem" },
  { metric: "container_restarts" as const, title: "ops.infra.charts.ctrRestarts" },
  { metric: "container_oom_events" as const, title: "ops.infra.charts.ctrOom" },
  { metric: "container_network_rx_bytes" as const, title: "ops.infra.charts.ctrNetRx" },
  { metric: "container_network_tx_bytes" as const, title: "ops.infra.charts.ctrNetTx" },
];

const { t } = useI18n();
const { resolveStep } = useStep();

const timeRange = ref<TimeRangeValue>({
  preset: "1h",
  start: Math.floor(Date.now() / 1000) - 3600,
  end: Math.floor(Date.now() / 1000),
});
const refreshInterval = ref(60);
const loading = ref(false);
const series = ref<Partial<Record<ChartMetric, MetricSeries>>>({});

async function reload() {
  loading.value = true;
  try {
    const { start, end } = timeRange.value;
    const step = resolveStep(end - start);
    const next: Partial<Record<ChartMetric, MetricSeries>> = {};
    await Promise.all(
      CHART_METRICS.map(async (metric) => {
        const res = await fetchMetricTimeseries({ metric, start, end, step });

      if (metric.startsWith("container_")) {
        next[metric] = {
          ...res,
          series: withContainerDisplayName(res.series || []),
        };
      } else {
        next[metric] = res;
      }
      }),
    );
    series.value = next;
  } catch (e) {
    ElMessage.error((e as Error).message || String(e));
  } finally {
    loading.value = false;
  }
}

onMounted(reload);
</script>

<style scoped>
.ops-infra__note {
  margin: 0 0 12px;
  color: var(--el-text-color-secondary);
  font-size: 13px;
}
</style>

