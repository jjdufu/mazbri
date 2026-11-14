<template>
  <OpsPage :title="title">
    <template #actions>
      <div class="ops-service-detail__actions">
        <ElSelect
          :model-value="serviceKey"
          filterable
          :placeholder="t('ops.services.switchService')"
          style="width: 220px"
          @change="handleSwitchService"
        >
          <ElOption
            v-for="opt in serviceOptions"
            :key="opt.value"
            :value="opt.value"
            :label="opt.label"
          />
        </ElSelect>
        <TimeRangePicker v-model="timeRange" @change="reload" />
        <AutoRefresh v-model="refreshInterval" @tick="reload" />
      </div>
    </template>

    <ElRow :gutter="16">
      <ElCol :xs="24" :sm="statColSpan">
        <StatCard
          :label="t('ops.services.detail.health')"
          :value="healthLabel"
          :loading="loading"
          icon="ri:heart-pulse-line"
        />
      </ElCol>
      <ElCol v-if="hasPostgresPool" :xs="24" :sm="statColSpan">
        <StatCard
          :label="t('ops.services.detail.dbPool')"
          :value="poolLabel"
          :loading="loading"
          icon="ri:database-2-line"
        />
      </ElCol>
      <ElCol :xs="24" :sm="statColSpan">
        <StatCard
          :label="t('ops.services.detail.version')"
          :value="row?.version || '—'"
          :loading="loading"
          icon="ri:git-commit-line"
        />
      </ElCol>
      <ElCol :xs="24" :sm="statColSpan">
        <StatCard
          :label="t('ops.services.detail.lastRestart')"
          :value="restartLabel"
          :loading="loading"
          icon="ri:restart-line"
        />
      </ElCol>
    </ElRow>

    <OpsCard :title="t('ops.services.detail.redTitle')">
      <ElRow :gutter="16">
        <ElCol
          v-for="red in redPanels"
          :key="red.key"
          :xs="24"
          :lg="8"
        >
          <div class="ops-service-detail__subcard">
            <div class="ops-service-detail__metric-title">{{ red.title }}</div>
            <EmptyState
              v-if="!loading && isSeriesEmpty(red.series)"
              compact
              :description="t('ops.services.detail.noTraffic')"
            />
            <MultiSeriesChart
              v-else
              :series="red.series"
              :loading="loading"
              :window="chartWindow"
              height="220px"
            />
          </div>
        </ElCol>
      </ElRow>
    </OpsCard>

    <OpsCard :title="t('ops.services.detail.useTitle')">
      <ElRow :gutter="16">
        <ElCol v-for="item in useMetrics" :key="item.key" :xs="24" :lg="8">
          <div class="ops-service-detail__subcard">
            <div class="ops-service-detail__metric-title">{{ item.label }}</div>
            <MultiSeriesChart
              :series="useSeries[item.key]"
              :loading="loading"
              :window="chartWindow"
              :unit-type="item.key === 'process_cpu_cores' ? 'cores' : undefined"
              height="200px"
            />
          </div>
        </ElCol>
      </ElRow>
    </OpsCard>

    <OpsCard :title="t('ops.services.detail.specific')">
      <EmptyState
        v-if="!specificMetrics.length"
        compact
        :description="t('ops.services.detail.noSpecific')"
      />
      <ElRow v-else :gutter="16">
        <ElCol
          v-for="item in specificMetrics"
          :key="item.key"
          :xs="24"
          :lg="12"
        >
          <div class="ops-service-detail__subcard">
            <div class="ops-service-detail__metric-title">{{ item.label }}</div>
            <MultiSeriesChart
              :series="specificSeries[item.key]"
              :loading="loading"
              :window="chartWindow"
              height="200px"
            />
          </div>
        </ElCol>
      </ElRow>
    </OpsCard>
  </OpsPage>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";
import { ElMessage } from "element-plus";
import OpsPage from "@/components/ops/OpsPage.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import EmptyState from "@/components/common/empty-state/index.vue";
import TimeRangePicker, {
  type TimeRangeValue,
} from "@/components/ops/TimeRangePicker.vue";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import StatCard from "@/components/ops/StatCard.vue";
import MultiSeriesChart from "@/components/ops/MultiSeriesChart.vue";
import {
  fetchMetricTimeseries,
  fetchMetricsCatalog,
  fetchServicesHealth,
} from "@/api/monitoring";
import type {
  MetricCatalogItem,
  MetricSeries,
  ServiceHealthRow,
} from "@/types/api/monitoring";
import { formatDateTime } from "@/utils/format";
import { buildServiceDetailRoute } from "@/utils/ops/services-link";
import {
  filterRowsByGroup,
  serviceDetailStatColSpan,
  serviceHasPostgresPool,
} from "@/utils/ops/service-topology";

defineOptions({ name: "OpsServiceDetail" });

const { t } = useI18n();
const route = useRoute();
const router = useRouter();

const serviceKey = computed(() => String(route.params.service || ""));
const loading = ref(false);
const allAppRows = ref<ServiceHealthRow[]>([]);
const row = ref<ServiceHealthRow | null>(null);
const qpsSeries = ref<MetricSeries | null>(null);
const errorSeries = ref<MetricSeries | null>(null);
const latencySeries = ref<MetricSeries | null>(null);
const specificSeries = ref<Record<string, MetricSeries | null>>({});
const useSeries = ref<Record<string, MetricSeries | null>>({});
const catalogMetrics = ref<MetricCatalogItem[]>([]);
const refreshInterval = ref(30);
const timeRange = ref<TimeRangeValue>({
  preset: "1h",
  start: Math.floor(Date.now() / 1000) - 3600,
  end: Math.floor(Date.now() / 1000),
});

const chartWindow = computed(() => ({
  start: timeRange.value.start,
  end: timeRange.value.end,
}));

const hasPostgresPool = computed(() =>
  serviceHasPostgresPool(serviceKey.value),
);
const statColSpan = computed(() =>
  serviceDetailStatColSpan(serviceKey.value),
);

const serviceOptions = computed(() => {
  const opts = allAppRows.value.map((item) => ({
    value: item.service,
    label: item.name
      ? `${item.name} (${item.service})`
      : item.service,
  }));
  const current = serviceKey.value.trim();
  if (current && !opts.some((o) => o.value === current)) {
    opts.unshift({ value: current, label: current });
  }
  return opts;
});

function handleSwitchService(nextService: string) {
  if (!nextService || nextService === serviceKey.value) return;
  const target = buildServiceDetailRoute(nextService);
  const preservedQuery: Record<string, string> = {};
  for (const key of ["tr_preset", "tr_start", "tr_end"] as const) {
    const val = route.query[key];
    if (typeof val === "string" && val) {
      preservedQuery[key] = val;
    }
  }
  if (typeof target === "object" && target !== null && Object.keys(preservedQuery).length > 0) {
    void router.push({ ...target, query: preservedQuery });
    return;
  }
  void router.push(target);
}

function isSeriesEmpty(series: MetricSeries | null | undefined): boolean {
  const list = series?.series ?? [];
  return list.reduce((sum, s) => sum + (s.points?.length ?? 0), 0) === 0;
}

const redPanels = computed(() => [
  {
    key: "qps",
    title: t("ops.services.detail.redQps"),
    series: qpsSeries.value,
  },
  {
    key: "errors",
    title: t("ops.services.detail.redErrors"),
    series: errorSeries.value,
  },
  {
    key: "latency",
    title: t("ops.services.detail.redLatencyP95"),
    series: latencySeries.value,
  },
]);

const title = computed(
  () => row.value?.name || serviceKey.value || t("menus.mazbri.serviceDetail"),
);
const healthLabel = computed(() => {
  if (!row.value) return "—";
  return row.value.health_ok ? t("ops.services.ok") : t("ops.services.down");
});
const poolLabel = computed(() => {
  if (
    row.value?.db_pool_usage === null ||
    row.value?.db_pool_usage === undefined
  ) {
    return "—";
  }
  return `${(row.value.db_pool_usage * 100).toFixed(1)}%`;
});

const restartLabel = computed(() =>
  row.value?.last_restart_ts
    ? formatDateTime(row.value.last_restart_ts)
    : "—",
);

/** USE 视图：所有服务共有的进程与 tokio 运行时指标（analytics 指标目录同名 key）。 */
const USE_METRIC_KEYS = [
  "process_cpu_cores",
  "process_resident_memory_mb",
  "process_open_fds",
  "tokio_busy_ratio",
  "tokio_alive_tasks",
  "tokio_scheduler_lag_p99_ms",
];

const useMetrics = computed(() =>
  USE_METRIC_KEYS.map((key) =>
    catalogMetrics.value.find((m) => m.key === key),
  ).filter((m): m is MetricCatalogItem => m !== undefined),
);

const specificPrefix = computed(() => {
  const key = serviceKey.value.replace(/^mazbri-/, "").replace(/-/g, "_");
  if (key === "gateway") return "gateway_";
  if (key === "auth") return "auth_";
  if (key === "cert") return "cert_";
  if (key === "billing") return "billing_";
  if (key === "cache") return "cache_";
  if (key === "bypass") return "bypass_";
  if (key.startsWith("cbpro")) return "cbpro_";
  return `${key}_`;
});

const specificMetrics = computed(() =>
  catalogMetrics.value
    .filter((m) => m.key.startsWith(specificPrefix.value))
    .slice(0, 4),
);

let reloadSeq = 0;

async function reload() {
  const currentService = serviceKey.value.trim();
  if (!currentService) return;
  const seq = ++reloadSeq;
  loading.value = true;
  try {
    const [healthRows, catalog, qps, errors, latency] = await Promise.all([
      fetchServicesHealth(),
      fetchMetricsCatalog(),
      fetchMetricTimeseries({
        metric: "service_qps",
        start: timeRange.value.start,
        end: timeRange.value.end,
        service: currentService,
      }).catch(() => null),
      fetchMetricTimeseries({
        metric: "error_rate",
        start: timeRange.value.start,
        end: timeRange.value.end,
        service: currentService,
      }).catch(() => null),
      fetchMetricTimeseries({
        metric: "p95_latency_ms",
        start: timeRange.value.start,
        end: timeRange.value.end,
        service: currentService,
      }).catch(() => null),
    ]);
    if (seq !== reloadSeq || serviceKey.value.trim() !== currentService) return;
    allAppRows.value = filterRowsByGroup(healthRows, "apps");
    row.value =
      healthRows.find((item) => item.service === currentService) || null;
    catalogMetrics.value = catalog.metrics || [];
    qpsSeries.value = qps;
    errorSeries.value = errors;
    latencySeries.value = latency;

    const next: Record<string, MetricSeries | null> = {};
    await Promise.all(
      specificMetrics.value.map(async (item) => {
        next[item.key] = await fetchMetricTimeseries({
          metric: item.key,
          start: timeRange.value.start,
          end: timeRange.value.end,
          service: currentService,
        }).catch(() => null);
      }),
    );
    if (seq !== reloadSeq || serviceKey.value.trim() !== currentService) return;
    specificSeries.value = next;

    const nextUse: Record<string, MetricSeries | null> = {};
    await Promise.all(
      useMetrics.value.map(async (item) => {
        nextUse[item.key] = await fetchMetricTimeseries({
          metric: item.key,
          start: timeRange.value.start,
          end: timeRange.value.end,
          service: currentService,
        }).catch(() => null);
      }),
    );
    if (seq !== reloadSeq || serviceKey.value.trim() !== currentService) return;
    useSeries.value = nextUse;
  } catch (error) {
    if (seq !== reloadSeq) return;
    ElMessage.error(
      error instanceof Error ? error.message : t("ops.services.loadFailed"),
    );
  } finally {
    if (seq === reloadSeq) {
      loading.value = false;
    }
  }
}

watch(serviceKey, () => {
  void reload();
});

onMounted(reload);
</script>

<style scoped>
.ops-service-detail__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}

.ops-service-detail__subcard {
  padding: 10px 12px;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: var(--el-border-radius-base, 6px);
  background: var(--el-bg-color);
  margin-bottom: 12px;
}

.ops-service-detail__metric-title {
  margin-bottom: 6px;
  font-size: 13px;
  font-weight: 600;
  color: var(--el-text-color-primary);
}
</style>
