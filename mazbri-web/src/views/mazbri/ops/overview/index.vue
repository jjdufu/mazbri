<template>
  <div class="ops-overview art-full-height">
    <div class="ops-overview__toolbar">
      <h2 class="ops-overview__title">{{ t("menus.mazbri.opsOverview") }}</h2>
      <div class="ops-overview__actions">
        <TimeRangePicker v-model="timeRange" @change="reload" />
        <AutoRefresh v-model="refreshInterval" @tick="reload" />
        <ElButton type="primary" @click="openScreen">
          <ArtSvgIcon icon="ri:tv-2-line" class="mr-1" />
          {{ t("ops.overview.openScreen") }}
        </ElButton>
      </div>
    </div>

    <ElAlert
      v-if="isViewerSlim"
      type="info"
      :closable="false"
      class="ops-overview__slim"
      :title="t('ops.overview.viewerSlim')"
    />

    <ElRow :gutter="16" class="ops-overview__cards">
      <ElCol :xs="24" :sm="12" :lg="6">
        <StatCard
          :label="t('ops.overview.stats.qps')"
          :value="overview?.qps"
          :loading="loading"
          unit="req/s"
          icon="ri:pulse-line"
          @click="drill('2.1')"
        />
      </ElCol>
      <ElCol :xs="24" :sm="12" :lg="6">
        <StatCard
          :label="t('ops.overview.stats.successRate')"
          :value="overview?.success_rate"
          unit="%"
          :loading="loading"
          icon="ri:checkbox-circle-line"
          icon-color="#67c23a"
          icon-background="rgba(103, 194, 58, 0.12)"
          @click="drill('2.4')"
        />
      </ElCol>
      <ElCol :xs="24" :sm="12" :lg="6">
        <StatCard
          :label="t('ops.overview.stats.latency')"
          :value="latencyLabel"
          :loading="loading"
          icon="ri:timer-line"
          icon-color="#e6a23c"
          icon-background="rgba(230, 162, 60, 0.12)"
          @click="drill('2.2')"
        />
      </ElCol>
      <ElCol :xs="24" :sm="12" :lg="6">
        <StatCard
          :label="t('ops.overview.stats.errorRates')"
          :value="errorLabel"
          :loading="loading"
          icon="ri:error-warning-line"
          icon-color="#f56c6c"
          icon-background="rgba(245, 108, 108, 0.12)"
          @click="drill('2.4')"
        />
      </ElCol>
    </ElRow>

    <ElRow :gutter="16" class="ops-overview__cards">
      <ElCol :xs="24" :sm="12" :lg="6">
        <StatCard
          :label="t('ops.overview.stats.circuitOpen')"
          :value="overview?.circuit_breaker_open_count"
          :loading="loading"
          icon="ri:shut-down-line"
          @click="drill('3.2')"
        />
      </ElCol>
      <ElCol :xs="24" :sm="12" :lg="6">
        <StatCard
          :label="t('ops.overview.stats.ratelimit')"
          :value="overview?.rate_limit_rejections_per_sec"
          unit="ops/s"
          :loading="loading"
          icon="ri:stop-circle-line"
          @click="drill('4.1')"
        />
      </ElCol>
      <ElCol :xs="24" :sm="12" :lg="6">
        <StatCard
          :label="t('ops.overview.stats.certExpiry')"
          :value="overview?.cert_min_expiry_days"
          unit="d"
          :loading="loading"
          icon="ri:shield-check-line"
          @click="drill('12.1')"
        />
      </ElCol>
      <ElCol :xs="24" :sm="12" :lg="6">
        <StatCard
          :label="t('ops.overview.stats.activeTenants')"
          :value="platform?.tenant_stats?.active_tenants"
          :loading="loading"
          :hide-amount="isViewerSlim"
          icon="ri:building-line"
        />
      </ElCol>
    </ElRow>

    <ElRow :gutter="16" class="ops-overview__cards">
      <ElCol
        v-for="card in billingCards"
        :key="card.key"
        :xs="12"
        :sm="6"
        :lg="6"
      >
        <StatCard
          :label="t(`ops.overview.stats.${card.key}`)"
          :value="overview?.billing_pipeline?.[card.field]"
          :unit="card.unit"
          :loading="loading"
          icon="ri:stack-line"
          @click="drill('6.1')"
        />
      </ElCol>
    </ElRow>

    <ElRow :gutter="16">
      <ElCol :xs="24" :lg="14">
        <ElCard shadow="never" class="ops-overview__panel">
          <template #header>
            <div class="ops-overview__panel-head">
              <span>{{ t("ops.overview.healthLights") }}</span>
              <ElButton
                link
                type="primary"
                @click="router.push('/ops/services')"
              >
                {{ t("ops.overview.openMatrix") }}
              </ElButton>
            </div>
          </template>
          <div class="ops-overview__lights">
            <button
              v-for="item in overview?.service_health || []"
              :key="item.service"
              type="button"
              class="ops-overview__light"
              :title="item.name"
              @click="router.push(`/ops/services/${item.service}`)"
            >
              <span class="ops-overview__dot" :data-status="item.status"></span>
              <span class="ops-overview__light-name">{{ item.name }}</span>
            </button>
          </div>
        </ElCard>
      </ElCol>
      <ElCol :xs="24" :lg="10">
        <ElCard shadow="never" class="ops-overview__alerts">
          <template #header>
            <div class="ops-overview__panel-head">
              <span>{{ t("ops.overview.alerts") }}</span>
              <ElButton
                link
                type="primary"
                @click="router.push('/ops/alerts/events')"
              >
                {{ t("ops.overview.alertsOpen") }}
              </ElButton>
            </div>
          </template>
          <div class="ops-overview__placeholder-grid">
            <ElTag
              :type="(alertSummary?.firing || 0) > 0 ? 'danger' : 'success'"
            >
              {{
                t("ops.overview.alertsFiring", { n: alertSummary?.firing ?? 0 })
              }}
            </ElTag>
            <ElTag type="warning">
              {{
                t("ops.overview.alertsAcked", { n: alertSummary?.acked ?? 0 })
              }}
            </ElTag>
          </div>
          <div
            v-if="alertSummary?.latest.length"
            class="ops-overview__alert-list"
          >
            <button
              v-for="item in alertSummary.latest"
              :key="item.id"
              type="button"
              class="ops-overview__alert-item"
              @click="
                router.push({
                  path: '/ops/alerts/events',
                  query: { event_id: item.id },
                })
              "
            >
              <span
                class="ops-overview__dot"
                :data-status="item.severity === 'critical' ? 'red' : 'yellow'"
              ></span>
              <span>{{ item.rule_name }}</span>
              <span class="ops-overview__alert-labels">{{
                alertLabels(item.labels)
              }}</span>
            </button>
          </div>
          <p v-else class="ops-overview__slim">
            {{ t("ops.overview.alertsNone") }}
          </p>
        </ElCard>
        <ElCard shadow="never" class="ops-overview__panel">
          <template #header>{{ t("ops.overview.placeholders") }}</template>
          <div class="ops-overview__placeholder-grid">
            <ElTag type="info">{{
              t("ops.overview.errorBudgetPending")
            }}</ElTag>
          </div>
          <div class="ops-overview__drilldowns">
            <ElButton
              v-for="item in drillTargets"
              :key="item.id"
              size="small"
              :disabled="!item.enabled"
              @click="item.enabled && router.push(item.path!)"
            >
              {{ item.label }}
            </ElButton>
          </div>
        </ElCard>
      </ElCol>
    </ElRow>

    <ElRow :gutter="16" class="ops-overview__charts">
      <ElCol :xs="24" :lg="12">
        <ElCard shadow="never">
          <template #header>{{ t("ops.overview.charts.qps") }}</template>
          <MultiSeriesChart
            :series="qpsSeries"
            :loading="loading"
            height="240px"
          />
        </ElCard>
      </ElCol>
      <ElCol :xs="24" :lg="12">
        <ElCard shadow="never">
          <template #header>{{ t("ops.overview.charts.p99") }}</template>
          <MultiSeriesChart
            v-loading="exemplarLoading"
            :series="p99Series"
            :loading="loading"
            height="240px"
            @point-click="onP99Click"
          />
        </ElCard>
      </ElCol>
    </ElRow>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import { ElMessage } from "element-plus";
import TimeRangePicker, {
  type TimeRangeValue,
} from "@/components/ops/TimeRangePicker.vue";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import StatCard from "@/components/ops/StatCard.vue";
import MultiSeriesChart from "@/components/ops/MultiSeriesChart.vue";
import { useDrilldown } from "@/composables/ops/useDrilldown";
import { fetchMetricTimeseries, fetchMetricsOverview } from "@/api/monitoring";
import { fetchPlatformStats } from "@/api/analytics";
import { fetchAlertSummary } from "@/api/alerts";
import type { AlertSummary } from "@/types/api/alerts";
import type { PlatformStats } from "@/types/api/analytics";
import type {
  BillingPipelineOverview,
  MetricOverview,
  MetricSeries,
} from "@/types/api/monitoring";
import { useUserStore } from "@/store/modules/user";
import { postOlapQuery } from "@/api/olap";
import type { ExemplarRow } from "@/types/api/olap";
import { buildTraceSearchRoute } from "@/utils/ops/trace-link";

defineOptions({ name: "OpsOverview" });

const { t } = useI18n();
const router = useRouter();
const { drilldown } = useDrilldown();
const userStore = useUserStore();

const loading = ref(false);
const overview = ref<MetricOverview | null>(null);
const platform = ref<PlatformStats | null>(null);
const qpsSeries = ref<MetricSeries | null>(null);
const p99Series = ref<MetricSeries | null>(null);
const exemplarLoading = ref(false);

/** P99 point click → exemplar requests at/above P99 around that point → slow trace list. */
async function onP99Click(point: { timestamp: number }) {
  const span = timeRange.value.end - timeRange.value.start;
  const windowSecs = Math.min(3600, Math.max(60, Math.ceil(span / 120)));
  exemplarLoading.value = true;
  try {
    const res = await postOlapQuery<ExemplarRow>({
      template: "exemplar_traces",
      params: { ts: point.timestamp, window_secs: windowSecs, limit: 5 },
    });
    const rows = res.rows.filter((r) => r.trace_id);
    if (!rows.length) {
      ElMessage.info(t("ops.traces.exemplarEmpty"));
      return;
    }
    const threshold = Math.floor(
      Math.min(...rows.map((r) => r.p99_ms || r.value_ms)),
    );
    void router.push(
      buildTraceSearchRoute({
        start: point.timestamp - windowSecs,
        end: point.timestamp + windowSecs,
        filters: { min_duration_ms: threshold },
      }),
    );
  } catch {
    ElMessage.error(t("ops.traces.loadFailed"));
  } finally {
    exemplarLoading.value = false;
  }
}
const alertSummary = ref<AlertSummary | null>(null);
const refreshInterval = ref(30);
const timeRange = ref<TimeRangeValue>({
  preset: "1h",
  start: Math.floor(Date.now() / 1000) - 3600,
  end: Math.floor(Date.now() / 1000),
});

const billingCards: {
  key: string;
  field: keyof BillingPipelineOverview;
  unit: string;
}[] = [
  { key: "billingBacklog", field: "backlog", unit: "" },
  { key: "billingOldestAge", field: "oldest_pending_age_seconds", unit: "s" },
  { key: "billingDeadLetters", field: "dead_letters", unit: "" },
  { key: "billingLagP99", field: "charge_lag_p99_ms", unit: "ms" },
];

const roles = computed(() => (userStore.info?.roles as string[]) || []);
const isViewerSlim = computed(
  () => roles.value.includes("R_VIEWER") && !roles.value.includes("R_ADMIN"),
);

const latencyLabel = computed(() => {
  const o = overview.value;
  if (!o) return "—";
  return `P50 ${fmt(o.p50_latency_ms)} / P95 ${fmt(o.p95_latency_ms)} / P99 ${fmt(o.p99_latency_ms)}`;
});
const errorLabel = computed(() => {
  const o = overview.value;
  if (!o) return "—";
  return `4xx ${fmt(o.error_rate_4xx)}% / 5xx ${fmt(o.error_rate_5xx)}%`;
});

const drillTargets = computed(() => [
  {
    id: "2.1",
    label: t("ops.overview.drill.gatewayRed"),
    enabled: true,
    path: "/ops/traffic/red",
  },
  {
    id: "2.2",
    label: t("ops.overview.drill.latency"),
    enabled: true,
    path: "/ops/traffic/latency",
  },
  {
    id: "2.4",
    label: t("ops.overview.drill.errors"),
    enabled: true,
    path: "/ops/traffic/errors",
  },
  {
    id: "3.2",
    label: t("ops.overview.drill.circuit"),
    enabled: true,
    path: "/ops/upstream/breaker",
  },
  { id: "4.1", label: t("ops.overview.drill.ratelimit"), enabled: false },
  {
    id: "6.1",
    label: t("ops.overview.drill.billing"),
    enabled: true,
    path: "/ops/billing/health",
  },
  { id: "12.1", label: t("ops.overview.drill.certs"), enabled: false },
  {
    id: "1.1",
    label: t("ops.overview.openMatrix"),
    enabled: true,
    path: "/ops/services",
  },
]);

function alertLabels(labels: Record<string, string>) {
  return Object.entries(labels)
    .filter(([key]) => key !== "category" && key !== "unit")
    .map(([key, value]) => `${key}=${value}`)
    .join(", ");
}

function fmt(n: number) {
  return new Intl.NumberFormat(undefined, { maximumFractionDigits: 2 }).format(
    n,
  );
}

function drill(id: string) {
  const target = drillTargets.value.find((item) => item.id === id);
  if (!target?.enabled || !target.path) {
    ElMessage.info(t("ops.overview.drillDisabled"));
    return;
  }
  drilldown({ path: target.path });
}

function openScreen() {
  const url = `${window.location.origin}${window.location.pathname}#/screen`;
  window.open(url, "_blank", "noopener,noreferrer");
}

async function reload() {
  loading.value = true;
  try {
    const [ov, plat, qps, p99, alerts] = await Promise.all([
      fetchMetricsOverview(),
      fetchPlatformStats({
        start_time: timeRange.value.start,
        end_time: timeRange.value.end,
      }).catch(() => null),
      fetchMetricTimeseries({
        metric: "qps",
        start: timeRange.value.start,
        end: timeRange.value.end,
      }).catch(() => null),
      fetchMetricTimeseries({
        metric: "p99_latency_ms",
        start: timeRange.value.start,
        end: timeRange.value.end,
      }).catch(() => null),
      fetchAlertSummary().catch(() => null),
    ]);
    overview.value = ov;
    platform.value = plat;
    qpsSeries.value = qps;
    p99Series.value = p99;
    alertSummary.value = alerts;
  } catch (error) {
    ElMessage.error(
      error instanceof Error ? error.message : t("ops.overview.loadFailed"),
    );
  } finally {
    loading.value = false;
  }
}

onMounted(reload);
</script>

<style scoped>
.ops-overview {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 16px;
}
.ops-overview__toolbar {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  align-items: center;
  justify-content: space-between;
}
.ops-overview__title {
  margin: 0;
  font-size: 20px;
  font-weight: 600;
}
.ops-overview__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}
.ops-overview__cards {
  margin-top: 0;
}
.ops-overview__slim {
  margin: 0;
}
.ops-overview__panel {
  min-height: 180px;
}
.ops-overview__alerts {
  margin-bottom: 16px;
}
.ops-overview__alert-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.ops-overview__alert-item {
  display: flex;
  gap: 8px;
  align-items: center;
  padding: 4px 0;
  border: none;
  background: transparent;
  cursor: pointer;
  text-align: left;
}
.ops-overview__alert-labels {
  overflow: hidden;
  color: var(--el-text-color-secondary);
  font-size: 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.ops-overview__panel-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.ops-overview__lights {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
}
.ops-overview__light {
  display: inline-flex;
  gap: 6px;
  align-items: center;
  padding: 6px 10px;
  border: 1px solid var(--el-border-color);
  border-radius: 999px;
  background: transparent;
  cursor: pointer;
}
.ops-overview__dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: #909399;
}
.ops-overview__dot[data-status="green"] {
  background: #67c23a;
}
.ops-overview__dot[data-status="yellow"] {
  background: #e6a23c;
}
.ops-overview__dot[data-status="red"] {
  background: #f56c6c;
}
.ops-overview__light-name {
  font-size: 12px;
}
.ops-overview__placeholder-grid {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-bottom: 12px;
}
.ops-overview__drilldowns {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.ops-overview__charts {
  margin-bottom: 8px;
}
</style>
