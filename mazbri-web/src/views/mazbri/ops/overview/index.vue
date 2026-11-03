<template>
  <OpsPage :title="t('menus.mazbri.opsOverview')">
    <template #actions>
      <TimeRangePicker v-model="timeRange" @change="reload" />
      <AutoRefresh v-model="refreshInterval" @tick="reload" />
      <ElButton type="primary" @click="openScreen">
        <ArtSvgIcon icon="ri:tv-2-line" class="mr-1" />
        {{ t("ops.overview.openScreen") }}
      </ElButton>
    </template>

    <p class="ops-overview__range-hint">
      {{ t("ops.overview.timeRangeHint") }}
    </p>

    <ElAlert
      v-if="isViewer"
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
          :interactive="canDrill('2.1')"
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
          :interactive="canDrill('2.4')"
          @click="drill('2.4')"
        />
      </ElCol>
      <ElCol :xs="24" :sm="12" :lg="6">
        <StatCard
          :label="t('ops.overview.stats.latency')"
          :value="latencyPrimary"
          unit="ms"
          :hint="latencySecondary"
          :loading="loading"
          icon="ri:timer-line"
          icon-color="#e6a23c"
          icon-background="rgba(230, 162, 60, 0.12)"
          :interactive="canDrill('2.2')"
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
          :interactive="canDrill('2.4')"
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
          :interactive="canDrill('3.2')"
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
          :interactive="canDrill('4.1')"
          @click="drill('4.1')"
        />
      </ElCol>
      <ElCol
        :xs="24"
        :sm="12"
        :lg="6"
        :class="{ 'ops-overview__tenant--muted': isViewer }"
      >
        <StatCard
          :label="t('ops.overview.stats.activeTenants')"
          :value="platform?.tenant_stats?.active_tenants"
          :loading="loading"
          icon="ri:building-line"
        />
      </ElCol>
    </ElRow>

    <RoleGate gate="biz">
      <ElRow :gutter="16" class="ops-overview__cards">
        <ElCol :xs="24" :sm="12" :lg="12">
          <StatCard
            :label="t('ops.overview.stats.billingBacklog')"
            :value="overview?.billing_pipeline?.backlog"
            :hint="billingBacklogHint"
            :loading="loading"
            icon="ri:stack-line"
            :interactive="canDrill('6.1')"
            @click="drill('6.1')"
          />
        </ElCol>
        <ElCol :xs="24" :sm="12" :lg="12">
          <StatCard
            :label="t('ops.overview.stats.billingDeadLetters')"
            :value="overview?.billing_pipeline?.dead_letters"
            :loading="loading"
            icon="ri:mail-forbid-line"
            :interactive="canDrill('6.1')"
            @click="drill('6.1')"
          />
        </ElCol>
      </ElRow>
    </RoleGate>

    <ElRow :gutter="16">
      <ElCol :xs="24" :lg="14">
        <ElCard shadow="never" class="ops-overview__panel">
          <template #header>
            <div class="ops-overview__panel-head">
              <span>{{ t("ops.overview.healthLights") }}</span>
              <ElButton
                link
                type="primary"
                @click="router.push(buildServicesRoute({ tab: 'health' }))"
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
              @click="openServiceHealth(item.service)"
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
                @click="router.push(buildAlertsRoute())"
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
            <RoleGate gate="admin">
              <CertExpiryBadge :days="overview?.cert_min_expiry_days" />
            </RoleGate>
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
                router.push(buildAlertsRoute({ eventId: item.id }))
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
            :on-point-click="p99ClickHandler"
          />
        </ElCard>
      </ElCol>
    </ElRow>

    <RoleGate gate="admin">
      <InfraHealthStrip ref="infraStripRef" />
    </RoleGate>

    <SloBudgetPanel
      ref="sloBudgetRef"
      :connected="overview?.error_budget_connected"
      :remaining="overview?.error_budget_remaining"
      :slo-name="overview?.error_budget_slo"
    />

    <ProductTopN ref="productTopNRef" :time-range="timeRange" />

    <RoleGate gate="biz">
      <RecentAnomalies ref="recentAnomaliesRef" />
      <RecentChanges ref="recentChangesRef" />
    </RoleGate>
  </OpsPage>
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
import OpsPage from "@/components/ops/OpsPage.vue";
import RoleGate from "@/components/ops/RoleGate.vue";
import StatCard from "@/components/ops/StatCard.vue";
import MultiSeriesChart, {
  type PointClick,
} from "@/components/ops/MultiSeriesChart.vue";
import { useDrilldown } from "@/composables/ops/useDrilldown";
import { useOpsRole } from "@/composables/ops/useOpsRole";
import { useRoleGatedLoader } from "@/composables/ops/useRoleGatedLoader";
import { fetchMetricTimeseries, fetchMetricsOverview } from "@/api/monitoring";
import { fetchPlatformStats } from "@/api/analytics";
import { fetchAlertSummary } from "@/api/alerts";
import type { AlertSummary } from "@/types/api/alerts";
import type { PlatformStats } from "@/types/api/analytics";
import type { MetricOverview, MetricSeries } from "@/types/api/monitoring";
import { postOlapQuery } from "@/api/olap";
import type { ExemplarRow } from "@/types/api/olap";
import {
  canOverviewDrill,
  resolveOverviewDrill,
  type OverviewDrillId,
} from "@/utils/ops/overview-drill";
import {
  buildAlertsRoute,
  buildCallsRoute,
  buildScreenHref,
  buildServiceDetailRoute,
  buildServicesRoute,
  buildTraceSearchRoute,
} from "@/utils/ops/opsRoutes";
import type { RoleGatedTask } from "@/utils/ops/role-gated-loader";
import CertExpiryBadge from "./modules/CertExpiryBadge.vue";
import InfraHealthStrip from "./modules/InfraHealthStrip.vue";
import ProductTopN from "./modules/ProductTopN.vue";
import RecentAnomalies from "./modules/RecentAnomalies.vue";
import RecentChanges from "./modules/RecentChanges.vue";
import SloBudgetPanel from "./modules/SloBudgetPanel.vue";

defineOptions({ name: "OpsOverview" });

type OverviewLoadKey = "overview" | "platform" | "qps" | "p99" | "alerts";

const OVERVIEW_LOAD_TASKS: readonly RoleGatedTask<OverviewLoadKey>[] = [
  { key: "overview", gate: "all" },
  { key: "platform", gate: "all" },
  { key: "qps", gate: "all" },
  { key: "p99", gate: "all" },
  { key: "alerts", gate: "all" },
];

const { t } = useI18n();
const router = useRouter();
const { drilldown } = useDrilldown();
const { isAdmin, isBiz, isViewer } = useOpsRole();

const overview = ref<MetricOverview | null>(null);
const platform = ref<PlatformStats | null>(null);
const qpsSeries = ref<MetricSeries | null>(null);
const p99Series = ref<MetricSeries | null>(null);
const alertSummary = ref<AlertSummary | null>(null);
const productTopNRef = ref<InstanceType<typeof ProductTopN> | null>(null);
const recentAnomaliesRef = ref<InstanceType<typeof RecentAnomalies> | null>(
  null,
);
const recentChangesRef = ref<InstanceType<typeof RecentChanges> | null>(null);
const infraStripRef = ref<InstanceType<typeof InfraHealthStrip> | null>(null);
const sloBudgetRef = ref<InstanceType<typeof SloBudgetPanel> | null>(null);
const exemplarLoading = ref(false);
const refreshInterval = ref(30);
const timeRange = ref<TimeRangeValue>({
  preset: "1h",
  start: Math.floor(Date.now() / 1000) - 3600,
  end: Math.floor(Date.now() / 1000),
});

const drillRole = computed(() => ({
  isAdmin: isAdmin.value,
  isBiz: isBiz.value,
  isViewer: isViewer.value,
}));

const latencyPrimary = computed(() => {
  const o = overview.value;
  if (!o) return null;
  return o.p99_latency_ms;
});

const latencySecondary = computed(() => {
  const o = overview.value;
  if (!o) return "";
  return t("ops.overview.latencySecondary", {
    p50: fmt(o.p50_latency_ms),
    p95: fmt(o.p95_latency_ms),
  });
});

const errorLabel = computed(() => {
  const o = overview.value;
  if (!o) return "—";
  return `4xx ${fmt(o.error_rate_4xx)}% / 5xx ${fmt(o.error_rate_5xx)}%`;
});

const billingBacklogHint = computed(() => {
  const age = overview.value?.billing_pipeline?.oldest_pending_age_seconds;
  if (age === undefined || age === null) return "";
  return t("ops.overview.billingOldestAgeHint", { age: fmt(age) });
});

/** VIEWER 关闭 exemplar；ADMIN → Trace；BIZ → 调用检索。 */
const p99ClickHandler = computed(() =>
  isViewer.value ? undefined : onP99Click,
);

const {
  loading,
  load: loadOverviewBundle,
} = useRoleGatedLoader<OverviewLoadKey, unknown>({
  tasks: OVERVIEW_LOAD_TASKS,
  run: async (key) => {
    switch (key) {
      case "overview":
        return fetchMetricsOverview();
      case "platform":
        return fetchPlatformStats({
          start_time: timeRange.value.start,
          end_time: timeRange.value.end,
        }).catch(() => null);
      case "qps":
        return fetchMetricTimeseries({
          metric: "qps",
          start: timeRange.value.start,
          end: timeRange.value.end,
        }).catch(() => null);
      case "p99":
        return fetchMetricTimeseries({
          metric: "p99_latency_ms",
          start: timeRange.value.start,
          end: timeRange.value.end,
        }).catch(() => null);
      case "alerts":
        return fetchAlertSummary().catch(() => null);
      default: {
        const _exhaustive: never = key;
        return _exhaustive;
      }
    }
  },
});

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

function canDrill(id: OverviewDrillId): boolean {
  return canOverviewDrill(id, drillRole.value);
}

function drill(id: OverviewDrillId) {
  const target = resolveOverviewDrill(id, drillRole.value);
  if (!target) return;
  drilldown({
    path: target.path,
    query: target.query,
  });
}

function openServiceHealth(service: string) {
  if (isAdmin.value) {
    void router.push(buildServiceDetailRoute(service));
    return;
  }
  void router.push(buildServicesRoute({ tab: "health" }));
}

async function onP99Click(point: PointClick) {
  if (isViewer.value) return;

  const span = timeRange.value.end - timeRange.value.start;
  const windowSecs = Math.min(3600, Math.max(60, Math.ceil(span / 120)));
  const start = point.timestamp - windowSecs;
  const end = point.timestamp + windowSecs;

  // 管理员（BIZ 非超管）：进调用检索，不打 exemplar / Trace。
  if (!isAdmin.value) {
    void router.push(
      buildCallsRoute({
        tab: "customer",
        start,
        end,
        filters: {
          duration_ms_gte: Math.max(0, Math.floor(point.value)),
        },
      }),
    );
    return;
  }

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
        start,
        end,
        filters: { min_duration_ms: threshold },
      }),
    );
  } catch {
    ElMessage.error(t("ops.traces.loadFailed"));
  } finally {
    exemplarLoading.value = false;
  }
}

function openScreen() {
  const url = buildScreenHref(window.location.origin, window.location.pathname);
  window.open(url, "_blank", "noopener,noreferrer");
}

async function reload() {
  try {
    const bundle = await loadOverviewBundle();
    overview.value = (bundle.overview as MetricOverview | undefined) ?? null;
    platform.value = (bundle.platform as PlatformStats | null | undefined) ?? null;
    qpsSeries.value = (bundle.qps as MetricSeries | null | undefined) ?? null;
    p99Series.value = (bundle.p99 as MetricSeries | null | undefined) ?? null;
    alertSummary.value =
      (bundle.alerts as AlertSummary | null | undefined) ?? null;
    await infraStripRef.value?.reload();
    await sloBudgetRef.value?.reload();
    await productTopNRef.value?.reload();
    await recentAnomaliesRef.value?.reload();
    await recentChangesRef.value?.reload();
  } catch (error) {
    ElMessage.error(
      error instanceof Error ? error.message : t("ops.overview.loadFailed"),
    );
  }
}

onMounted(reload);
</script>

<style scoped>
.ops-overview__range-hint {
  margin: 0 0 12px;
  color: var(--el-text-color-secondary);
  font-size: 12px;
  line-height: 1.4;
}
.ops-overview__cards {
  margin-top: 0;
}
.ops-overview__cards :deep(.el-col) {
  display: flex;
}
.ops-overview__cards :deep(.el-col > *) {
  width: 100%;
}
.ops-overview__slim {
  margin: 0;
}
.ops-overview__tenant--muted :deep(.ops-stat-card__value) {
  color: var(--el-text-color-regular);
  font-weight: 500;
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
.ops-overview__charts {
  margin-bottom: 8px;
}
</style>
