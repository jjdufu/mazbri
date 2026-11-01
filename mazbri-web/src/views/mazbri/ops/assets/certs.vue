<template>
  <div class="ops-traffic art-full-height">
    <div class="ops-traffic__toolbar">
      <h2>{{ t("menus.mazbri.assetsCerts") }}</h2>
      <div class="ops-traffic__actions">
        <TimeRangePicker v-model="timeRange" @change="reload" />
        <AutoRefresh v-model="refreshInterval" @tick="reload" />
      </div>
    </div>

    <ElRow :gutter="12">
      <ElCol v-for="chart in trendCharts" :key="chart.metric" :xs="24" :lg="12">
        <div class="ops-traffic__card">
          <h3>{{ t(chart.title) }}</h3>
          <MultiSeriesChart
            :series="chart.data"
            :loading="loading"
            :annotations="showChanges ? annotations : []"
            height="220px"
          />
        </div>
      </ElCol>
    </ElRow>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import TimeRangePicker, {
  type TimeRangeValue,
} from "@/components/ops/TimeRangePicker.vue";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import MultiSeriesChart, {
  type ChartAnnotation,
} from "@/components/ops/MultiSeriesChart.vue";
import { useStep } from "@/composables/ops/useStep";
import { fetchMetricTimeseries } from "@/api/monitoring";
import { fetchOpsChanges } from "@/api/opsChanges";
import type { MetricSeries } from "@/types/api/monitoring";

defineOptions({ name: "OpsAssetsCerts" });

const METRICS = [
  "cert_active_count",
  "cert_issuance",
  "tls_cert_expiry_days",
  "cert_min_expiry_days",
  "license_expiry_days",
  "license_usage_ratio",
] as const;
type Metric = (typeof METRICS)[number];

const { t } = useI18n();
const { resolveStep } = useStep();
const timeRange = ref<TimeRangeValue>({
  preset: "24h",
  start: Math.floor(Date.now() / 1000) - 86400,
  end: Math.floor(Date.now() / 1000),
});
const refreshInterval = ref(60);
const loading = ref(false);
const showChanges = ref(true);
const annotations = ref<ChartAnnotation[]>([]);
const series = ref<Record<Metric, MetricSeries | null>>(
  Object.fromEntries(METRICS.map((m) => [m, null])) as Record<
    Metric,
    MetricSeries | null
  >,
);

const trendCharts = computed(() => [
  { metric: "active", title: "ops.assets.certActive", data: series.value.cert_active_count },
  { metric: "issuance", title: "ops.assets.certIssuance", data: series.value.cert_issuance },
  { metric: "tls", title: "ops.assets.tlsExpiry", data: series.value.tls_cert_expiry_days },
  { metric: "min", title: "ops.assets.certMinExpiry", data: series.value.cert_min_expiry_days },
  { metric: "lic", title: "ops.assets.licenseExpiry", data: series.value.license_expiry_days },
  { metric: "usage", title: "ops.assets.licenseUsage", data: series.value.license_usage_ratio },
]);

async function reload() {
  loading.value = true;
  try {
    const step = resolveStep(timeRange.value.end - timeRange.value.start);
    const results = await Promise.all(
      METRICS.map((metric) =>
        fetchMetricTimeseries({
          metric,
          start: timeRange.value.start,
          end: timeRange.value.end,
          step,
        }).catch(() => null),
      ),
    );
    METRICS.forEach((m, i) => {
      series.value[m] = results[i];
    });
    const changes = await fetchOpsChanges({
      start: timeRange.value.start,
      end: timeRange.value.end,
      limit: 50,
    }).catch(() => ({ events: [] }));
    annotations.value = (changes?.events || [])
      .filter((e) => e.kind === "license" || e.kind === "publish" || e.kind === "route")
      .map((e) => ({
        timestamp: Math.floor(new Date(e.ts).getTime() / 1000),
        label: e.title,
        color: "#e6a23c",
      }));
  } catch (e) {
    ElMessage.error(String(e));
  } finally {
    loading.value = false;
  }
}

onMounted(reload);
</script>

<style scoped>
.ops-traffic__toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 12px;
}
.ops-traffic__actions {
  display: flex;
  gap: 8px;
  align-items: center;
}
.ops-traffic__card {
  background: var(--el-bg-color);
  border-radius: 8px;
  padding: 12px;
  margin-bottom: 12px;
}
</style>
