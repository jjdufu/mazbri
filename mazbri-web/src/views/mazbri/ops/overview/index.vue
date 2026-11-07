<template>
  <OpsPage :title="t('menus.mazbri.opsOverview')" :subtitle="windowSemantics">
    <template #actions>
      <div class="ops-overview__filter" data-role="global-filter">
        <TimeRangePicker v-model="timeRange" @change="reload" />
        <AutoRefresh v-model="refreshInterval" @tick="reload" />
      </div>
    </template>

    <ElAlert
      v-if="isViewer"
      type="info"
      :closable="false"
      class="ops-overview__slim"
      :title="t('ops.overview.viewerSlim')"
    />

    <!-- KPI：跟随时间窗（窗口汇总） -->
    <OpsSection
      id="kpi"
      :title="t('ops.overview.sections.kpi')"
    >
      <OpsKpiRow>
        <ElCol v-bind="KPI_COL">
          <StatCard
            :label="t('ops.overview.stats.qps')"
            :value="traffic?.qps"
            :unit="t('ops.units.perSec')"
            :loading="loading"
            icon="ri:pulse-line"
            :interactive="canDrill('2.1')"
            @click="drill('2.1')"
          />
        </ElCol>
        <ElCol v-bind="KPI_COL">
          <StatCard
            :label="t('ops.overview.stats.successRate')"
            :value="traffic?.success_rate"
            :unit="t('ops.units.percent')"
            :loading="loading"
            icon="ri:checkbox-circle-line"
            icon-color="#67c23a"
            icon-background="rgba(103, 194, 58, 0.12)"
            :interactive="canDrill('2.4')"
            @click="drill('2.4')"
          />
        </ElCol>
        <ElCol v-bind="KPI_COL">
          <StatCard
            :label="t('ops.overview.stats.latency')"
            :value="traffic?.p99_latency_ms"
            :unit="t('ops.units.ms')"
            :hint="latencyHint(traffic)"
            :loading="loading"
            icon="ri:timer-line"
            icon-color="#e6a23c"
            icon-background="rgba(230, 162, 60, 0.12)"
            :interactive="canDrill('2.2')"
            @click="drill('2.2')"
          />
        </ElCol>
        <ElCol v-bind="KPI_COL">
          <StatCard
            :label="t('ops.overview.stats.error4xx')"
            :value="traffic?.error_rate_4xx"
            :unit="t('ops.units.percent')"
            :loading="loading"
            icon="ri:error-warning-line"
            icon-color="#e6a23c"
            icon-background="rgba(230, 162, 60, 0.12)"
            :interactive="canDrill('2.4')"
            @click="drill('2.4')"
          />
        </ElCol>
        <ElCol v-bind="KPI_COL">
          <StatCard
            :label="t('ops.overview.stats.error5xx')"
            :value="traffic?.error_rate_5xx"
            :unit="t('ops.units.percent')"
            :loading="loading"
            icon="ri:close-circle-line"
            icon-color="#f56c6c"
            icon-background="rgba(245, 108, 108, 0.12)"
            :interactive="canDrill('2.4')"
            @click="drill('2.4')"
          />
        </ElCol>
        <ElCol v-bind="KPI_COL">
          <StatCard
            :label="t('ops.overview.stats.ratelimit')"
            :value="traffic?.rate_limit_rejections_per_sec"
            :unit="t('ops.units.perSec')"
            :loading="loading"
            icon="ri:stop-circle-line"
            :interactive="canDrill('4.1')"
            @click="drill('4.1')"
          />
        </ElCol>
      </OpsKpiRow>
    </OpsSection>

    <!-- 实时状态：此刻（不随时间窗） -->
    <OpsSection
      id="realtime"
      :title="t('ops.overview.sections.realtime')"
      :semantics="realtimeSemantics"
    >
      <OpsKpiRow>
        <ElCol v-bind="RT_COL">
          <StatCard
            :label="t('ops.overview.stats.proxyConnections')"
            :value="realtime?.proxy_active_connections"
            :loading="loading"
            :digits="0"
            icon="ri:links-line"
          />
        </ElCol>
        <ElCol v-bind="RT_COL">
          <StatCard
            :label="t('ops.overview.stats.circuitOpen')"
            :value="realtime?.circuit_breaker_open_count"
            :loading="loading"
            :digits="0"
            icon="ri:shut-down-line"
            :interactive="canDrill('3.2')"
            @click="drill('3.2')"
          />
        </ElCol>
        <ElCol v-bind="RT_COL" :class="{ 'ops-overview__tenant--muted': isViewer }">
          <StatCard
            :label="t('ops.overview.stats.enabledTenants')"
            :value="inventory?.tenants.enabled"
            :hint="inventoryHint"
            :loading="loading"
            :digits="0"
            icon="ri:building-line"
          />
        </ElCol>
        <RoleGate gate="biz">
          <ElCol v-bind="RT_COL">
            <StatCard
              :label="t('ops.overview.stats.billingBacklog')"
              :value="realtime?.billing_pipeline?.backlog"
              :hint="billingBacklogHint"
              :loading="loading"
              :digits="0"
              icon="ri:stack-line"
              :interactive="canDrill('6.1')"
              @click="drill('6.1')"
            />
          </ElCol>
        </RoleGate>
      </OpsKpiRow>
    </OpsSection>

    <!-- 活跃告警：独占一行 -->
    <OpsSection
      id="active-alerts"
      :title="t('ops.overview.alerts')"
    >
      <template #extra>
        <ElButton link type="primary" @click="router.push(buildAlertsRoute())">
          {{ t("ops.overview.alertsOpen") }}
        </ElButton>
      </template>
      <OpsCard class="ops-main-panel" data-block="alerts">
        <div class="ops-overview__tags">
          <ElTag :type="(alertSummary?.firing || 0) > 0 ? 'danger' : 'success'">
            {{ t("ops.overview.alertsFiring", { n: alertSummary?.firing ?? 0 }) }}
          </ElTag>
          <RoleGate gate="admin">
            <CertExpiryBadge :days="realtime?.cert_min_expiry_days" />
          </RoleGate>
        </div>
        <div v-if="alertSummary?.latest.length" class="ops-overview__alert-list">
          <button
            v-for="item in alertSummary.latest"
            :key="item.id"
            type="button"
            class="ops-overview__alert-item"
            @click="router.push(buildAlertsRoute({ eventId: item.id }))"
          >
            <span
              class="ops-overview__dot"
              :data-status="item.severity === 'critical' ? 'red' : 'yellow'"
            ></span>
            <span>{{ item.rule_name }}</span>
            <span class="ops-overview__alert-labels">{{ alertLabels(item.labels) }}</span>
          </button>
        </div>
        <EmptyState v-else compact :description="t('ops.overview.alertsNone')" />
      </OpsCard>
    </OpsSection>

    <!-- 流量质量摘要：轻图下钻调用日志（禁止整页搬入 traffic） -->
    <OpsSection
      id="traffic-quality"
      :title="t('ops.overview.sections.trafficQuality')"
    >
      <template #extra>
        <ElButton link type="primary" @click="openCallsWindow">
          {{ t("ops.overview.trafficQualityOpenCalls") }}
        </ElButton>
      </template>
      <ElRow :gutter="16" class="ops-equal-row">
        <ElCol :xs="24" :lg="12">
          <OpsCard :title="t('ops.overview.charts.qps')" class="ops-main-panel">
            <MultiSeriesChart
              :series="qpsSeries"
              :loading="loading"
              :window="chartWindow"
              height="240px"
              :on-point-click="qpsClickHandler"
            />
          </OpsCard>
        </ElCol>
        <ElCol :xs="24" :lg="12">
          <OpsCard :title="t('ops.overview.charts.p99')" class="ops-main-panel">
            <MultiSeriesChart
              v-loading="exemplarLoading"
              :series="p99Series"
              :loading="loading"
              :window="chartWindow"
              height="240px"
              :on-point-click="p99ClickHandler"
            />
          </OpsCard>
        </ElCol>
      </ElRow>
    </OpsSection>

    <!-- 安全与治理摘要 -->
    <OpsSection
      id="security-gov"
      :title="t('ops.overview.sections.securityGov')"
    >
      <SecurityGovSummary
        ref="securityGovRef"
        :start="chartWindow?.start ?? 0"
        :end="chartWindow?.end ?? 0"
      />
    </OpsSection>

    <!-- 洞察：跟随时间窗（SLO 为其自身评估窗口，单独标注） -->
    <OpsSection
      id="insight"
      :title="t('ops.overview.sections.insight')"
    >
      <ProductTopN ref="productTopNRef" />
      <RoleGate gate="biz">
        <RecentAnomalies ref="recentAnomaliesRef" />
      </RoleGate>
      <SloBudgetPanel
        ref="sloBudgetRef"
        :connected="overview?.error_budget_connected"
        :remaining="overview?.error_budget_remaining"
        :slo-name="overview?.error_budget_slo"
      />
    </OpsSection>
  </OpsPage>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import { ElMessage } from "element-plus";
import TimeRangePicker, { type TimeRangeValue } from "@/components/ops/TimeRangePicker.vue";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import OpsKpiRow from "@/components/ops/OpsKpiRow.vue";
import OpsPage from "@/components/ops/OpsPage.vue";
import OpsSection from "@/components/ops/OpsSection.vue";
import RoleGate from "@/components/ops/RoleGate.vue";
import StatCard from "@/components/ops/StatCard.vue";
import EmptyState from "@/components/common/empty-state/index.vue";
import MultiSeriesChart, { type PointClick } from "@/components/ops/MultiSeriesChart.vue";
import { useDrilldown } from "@/composables/ops/useDrilldown";
import { useOpsRole } from "@/composables/ops/useOpsRole";
import { resolveStep } from "@/composables/ops/useStep";
import { fetchMetricTimeseries, fetchMetricsOverview } from "@/api/monitoring";
import { fetchPlatformInventory } from "@/api/analytics";
import { fetchAlertSummary } from "@/api/alerts";
import { postOlapQuery } from "@/api/olap";
import type { AlertSummary } from "@/types/api/alerts";
import type { PlatformInventory } from "@/types/api/analytics";
import type { MetricOverview, MetricSeries, TrafficKpis } from "@/types/api/monitoring";
import type { ExemplarRow } from "@/types/api/olap";
import { formatWallClockHm, formatWallClockMdHm } from "@/utils/format";
import {
  canOverviewDrill,
  resolveOverviewDrill,
  type OverviewDrillId,
} from "@/utils/ops/overview-drill";
import {
  buildAlertsRoute,
  buildCallsRoute,
  buildTraceSearchRoute,
} from "@/utils/ops/opsRoutes";
import { defaultTimeRange, resolveRange } from "@/utils/ops/time-range";
import CertExpiryBadge from "./modules/CertExpiryBadge.vue";
import ProductTopN from "./modules/ProductTopN.vue";
import RecentAnomalies from "./modules/RecentAnomalies.vue";
import SecurityGovSummary from "./modules/SecurityGovSummary.vue";
import SloBudgetPanel from "./modules/SloBudgetPanel.vue";

defineOptions({ name: "OpsOverview" });

/** KPI 六卡一行（lg）；窄屏 3/2/1 列，保证每行等宽对齐。 */
const KPI_COL = { xs: 24, sm: 12, md: 8, lg: 4 } as const;
/** 实时状态四卡一行（lg）；观察者少一卡时仍左对齐。 */
const RT_COL = { xs: 24, sm: 12, md: 6, lg: 6 } as const;

/** 时间窗下钻：目标页继承同一 tr_*；实时类下钻（熔断/计费）不带窗。 */
const WINDOWED_DRILLS: ReadonlySet<OverviewDrillId> = new Set(["2.1", "2.2", "2.4", "4.1"]);

const { t } = useI18n();
const router = useRouter();
const { drilldown } = useDrilldown();
const { isAdmin, isBiz, isViewer } = useOpsRole();

const overview = ref<MetricOverview | null>(null);
const inventory = ref<PlatformInventory | null>(null);
const qpsSeries = ref<MetricSeries | null>(null);
const p99Series = ref<MetricSeries | null>(null);
const alertSummary = ref<AlertSummary | null>(null);
const productTopNRef = ref<InstanceType<typeof ProductTopN> | null>(null);
const recentAnomaliesRef = ref<InstanceType<typeof RecentAnomalies> | null>(null);
const securityGovRef = ref<InstanceType<typeof SecurityGovSummary> | null>(null);
const sloBudgetRef = ref<InstanceType<typeof SloBudgetPanel> | null>(null);
const loading = ref(false);
const exemplarLoading = ref(false);
const refreshInterval = ref(30);
/** 选择器值（预设或自定义）；URL 带 tr_* 时由 TimeRangePicker 挂载回填，并触发首轮 change。 */
const timeRange = ref<TimeRangeValue>(defaultTimeRange("6h"));
/** 本轮实际求值的窗口（预设按「现在」滚动重算），图表 x 轴与之对齐。 */
const chartWindow = ref<{ start: number; end: number } | null>(null);
let seq = 0;

const traffic = computed(() => overview.value?.traffic ?? null);
const realtime = computed(() => overview.value?.realtime ?? null);

const drillRole = computed(() => ({
  isAdmin: isAdmin.value,
  isBiz: isBiz.value,
  isViewer: isViewer.value,
}));

function presetLabel(): string {
  return t(`ops.timeRange.presets.${timeRange.value.preset}`);
}

/** 「跟随时间窗 · 11-05 09:00 – 11-05 15:00（6 小时）」——取后端回显的实际窗口。 */
const windowSemantics = computed(() => {
  const w = overview.value?.window ?? chartWindow.value;
  if (!w) return t("ops.overview.followsWindow");
  return t("ops.overview.windowSemantics", {
    start: formatWallClockMdHm(w.start * 1000),
    end: formatWallClockMdHm(w.end * 1000),
    label: presetLabel(),
  });
});

/** 「此刻 · 近 60 秒 · 截至 15:00」 */
const realtimeSemantics = computed(() => {
  const r = realtime.value;
  if (!r) return t("ops.overview.realtimeNow");
  return t("ops.overview.realtimeSemantics", {
    secs: r.lookback_secs,
    at: formatWallClockHm(r.eval_at * 1000),
  });
});

function latencyHint(k: TrafficKpis | null): string {
  if (!k || k.p99_latency_ms == null) return "";
  return t("ops.overview.latencySecondary", {
    p50: fmt(k.p50_latency_ms),
    p95: fmt(k.p95_latency_ms),
  });
}

const inventoryHint = computed(() => {
  const inv = inventory.value;
  if (!inv) return "";
  return t("ops.overview.inventoryHint", {
    total: inv.tenants.total,
    products: inv.products.enabled,
  });
});

const billingBacklogHint = computed(() => {
  const age = realtime.value?.billing_pipeline?.oldest_pending_age_seconds;
  if (age === undefined || age === null) return "";
  return t("ops.overview.billingOldestAgeHint", { age: fmt(age) });
});

/** VIEWER 关闭 exemplar；ADMIN → Trace；BIZ → 调用检索。 */
const p99ClickHandler = computed(() => (isViewer.value ? undefined : onP99Click));
const qpsClickHandler = computed(() => (isViewer.value ? undefined : onQpsClick));

function openCallsWindow() {
  const w = chartWindow.value;
  if (!w) {
    void router.push(buildCallsRoute({ tab: "customer" }));
    return;
  }
  void router.push(
    buildCallsRoute({ tab: "customer", start: w.start, end: w.end }),
  );
}

function onQpsClick(point: PointClick) {
  if (isViewer.value) return;
  const w = chartWindow.value;
  const span = w ? w.end - w.start : 3600;
  const windowSecs = Math.min(3600, Math.max(60, Math.ceil(span / 120)));
  void router.push(
    buildCallsRoute({
      tab: "customer",
      start: point.timestamp - windowSecs,
      end: point.timestamp + windowSecs,
    }),
  );
}

function alertLabels(labels: Record<string, string>) {
  return Object.entries(labels)
    .filter(([key]) => key !== "category" && key !== "unit")
    .map(([key, value]) => `${key}=${value}`)
    .join(", ");
}

function fmt(n: number | null | undefined) {
  if (n === null || n === undefined || Number.isNaN(n)) return "N/A";
  return new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 2 }).format(n);
}

function canDrill(id: OverviewDrillId): boolean {
  return canOverviewDrill(id, drillRole.value);
}

function drill(id: OverviewDrillId) {
  const target = resolveOverviewDrill(id, drillRole.value);
  if (!target) return;
  const query: Record<string, string> = { ...target.query };
  const w = chartWindow.value;
  if (WINDOWED_DRILLS.has(id) && w) {
    query.tr_preset = timeRange.value.preset;
    query.tr_start = String(w.start);
    query.tr_end = String(w.end);
  }
  if (target.path === "/ops/calls" && !query.tab) {
    query.tab = "customer";
  }
  drilldown({ path: target.path, query });
}

async function onP99Click(point: PointClick) {
  if (isViewer.value) return;
  const w = chartWindow.value;
  const span = w ? w.end - w.start : 3600;
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
        filters: { duration_ms_gte: Math.max(0, Math.floor(point.value)) },
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
    const threshold = Math.floor(Math.min(...rows.map((r) => r.p99_ms || r.value_ms)));
    void router.push(
      buildTraceSearchRoute({ start, end, filters: { min_duration_ms: threshold } }),
    );
  } catch {
    ElMessage.error(t("ops.traces.loadFailed"));
  } finally {
    exemplarLoading.value = false;
  }
}

/**
 * 整页刷新：每轮按「现在」解析一次窗口，所有跟窗块共用同一 [start, end)。
 * 序号守卫：快速切换时间窗时只采用最后一轮的响应，旧响应直接丢弃。
 */
async function reload() {
  const mine = ++seq;
  const r = resolveRange(timeRange.value);
  const win = { start: r.start, end: r.end };
  const step = resolveStep(win.end - win.start);
  loading.value = true;

  void productTopNRef.value?.reload(win);
  void recentAnomaliesRef.value?.reload(win);
  void securityGovRef.value?.reload();
  void sloBudgetRef.value?.reload();

  try {
    const [ov, inv, qps, p99, alerts] = await Promise.all([
      fetchMetricsOverview(win),
      fetchPlatformInventory().catch(() => null),
      fetchMetricTimeseries({ metric: "qps", ...win, step }).catch(() => null),
      fetchMetricTimeseries({ metric: "p99_latency_ms", ...win, step }).catch(() => null),
      fetchAlertSummary().catch(() => null),
    ]);
    if (mine !== seq) return;
    chartWindow.value = win;
    overview.value = ov ?? null;
    inventory.value = inv;
    qpsSeries.value = qps;
    p99Series.value = p99;
    alertSummary.value = alerts;
  } catch (error) {
    if (mine !== seq) return;
    ElMessage.error(error instanceof Error ? error.message : t("ops.overview.loadFailed"));
  } finally {
    if (mine === seq) loading.value = false;
  }
}
</script>

<style scoped>
.ops-overview__filter {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--ops-space-xs, 8px);
}
.ops-overview__slim {
  margin: 0;
}
.ops-overview__tenant--muted :deep(.ops-stat-card__value) {
  color: var(--el-text-color-regular);
  font-weight: 500;
}
.ops-overview__tags {
  display: flex;
  flex-wrap: wrap;
  gap: var(--ops-space-xs, 8px);
  margin-bottom: var(--ops-space-sm, 12px);
}
.ops-overview__alert-list {
  display: flex;
  flex-direction: column;
  gap: var(--ops-space-xs, 8px);
}
.ops-overview__alert-item {
  display: flex;
  gap: var(--ops-space-xs, 8px);
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
.ops-overview__dot {
  flex: none;
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: #909399;
}
.ops-overview__dot[data-status="yellow"] {
  background: #e6a23c;
}
.ops-overview__dot[data-status="red"] {
  background: #f56c6c;
}
</style>
