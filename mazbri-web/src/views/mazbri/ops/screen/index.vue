<template>
  <div class="ops-screen">
    <header class="ops-screen__header">
      <h1>{{ t("ops.screen.title") }}</h1>
      <div class="ops-screen__meta">
        <div class="ops-screen__tabs">
          <button
            v-for="p in panels"
            :key="p"
            type="button"
            class="ops-screen__tab"
            :class="{ 'is-active': panel === p }"
            @click="panel = p"
          >
            {{ t(`ops.screen.panels.${p}`) }}
          </button>
        </div>
        <span>{{ clock }}</span>
        <AutoRefresh v-model="refreshInterval" @tick="loadAll" />
        <button
          type="button"
          class="ops-screen__exit"
          :aria-label="t('ops.screen.exitToDashboard')"
          @click="exitToDashboard"
        >
          {{ t("ops.screen.exitToDashboard") }}
        </button>
      </div>
    </header>

    <div v-show="panel === 'overview'" class="ops-screen__panel">
      <ElRow :gutter="16">
        <ElCol :span="6"><ScreenStat :label="t('ops.screen.qps')" :value="fmt(overview?.qps)" /></ElCol>
        <ElCol :span="6"><ScreenStat :label="t('ops.screen.successRate')" :value="pct(overview?.success_rate)" /></ElCol>
        <ElCol :span="6"><ScreenStat :label="t('ops.screen.p99')" :value="ms(overview?.p99_latency_ms)" /></ElCol>
        <ElCol :span="6"><ScreenStat :label="t('ops.screen.burn')" :value="burnLabel" /></ElCol>
      </ElRow>
      <ElRow :gutter="16" class="ops-screen__row">
        <ElCol :span="14">
          <div class="ops-screen__card">
            <div class="ops-screen__label">{{ t("ops.screen.qps") }} · 24h</div>
            <div ref="qpsRef" class="ops-screen__chart" />
          </div>
        </ElCol>
        <ElCol :span="10">
          <div class="ops-screen__card">
            <div class="ops-screen__label">{{ t("ops.screen.topology") }}</div>
            <div ref="topoRef" class="ops-screen__chart ops-screen__chart--sm" />
          </div>
        </ElCol>
      </ElRow>
      <ElRow :gutter="16" class="ops-screen__row">
        <ElCol :span="24">
          <div class="ops-screen__card ops-screen__marquee">
            <div class="ops-screen__label">{{ t("ops.screen.alerts") }}</div>
            <div class="ops-screen__marquee-track">{{ alertText }}</div>
          </div>
        </ElCol>
      </ElRow>
    </div>

    <div v-show="panel === 'business'" class="ops-screen__panel">
      <ElRow :gutter="16">
        <ElCol :span="8"><ScreenStat :label="t('ops.screen.revenue')" :value="money(todayRevenue)" /></ElCol>
        <ElCol :span="8"><ScreenStat :label="t('ops.screen.cost')" :value="money(todayCost)" /></ElCol>
        <ElCol :span="8"><ScreenStat :label="t('ops.screen.margin')" :value="money(todayMargin)" /></ElCol>
      </ElRow>
      <ElRow :gutter="16" class="ops-screen__row">
        <ElCol :span="12">
          <div class="ops-screen__card">
            <div class="ops-screen__label">{{ t("ops.screen.topProducts") }}</div>
            <ul class="ops-screen__list">
              <li v-for="(row, i) in topProducts" :key="i">
                <span>{{ row.name }}</span><strong>{{ fmt(row.value) }}</strong>
              </li>
              <li v-if="!topProducts.length" class="is-empty">{{ t("ops.screen.noData") }}</li>
            </ul>
          </div>
        </ElCol>
        <ElCol :span="12">
          <div class="ops-screen__card">
            <div class="ops-screen__label">{{ t("ops.screen.supplierSla") }}</div>
            <div class="ops-screen__sla">
              <div
                v-for="s in supplierWall"
                :key="s.name"
                class="ops-screen__sla-cell"
                :class="s.tone"
                :title="s.name"
              >
                {{ s.name }}
              </div>
              <div v-if="!supplierWall.length" class="is-empty">{{ t("ops.screen.noData") }}</div>
            </div>
          </div>
        </ElCol>
      </ElRow>
      <ElRow :gutter="16" class="ops-screen__row">
        <ElCol :span="24">
          <div class="ops-screen__card">
            <div class="ops-screen__label">{{ t("ops.screen.geo") }}</div>
            <div class="ops-screen__geo">
              <div v-for="g in geoRows" :key="g.dim" class="ops-screen__geo-row">
                <span>{{ g.dim }}</span>
                <div class="ops-screen__bar"><i :style="{ width: g.width }" /></div>
                <strong>{{ fmt(g.count) }}</strong>
              </div>
              <div v-if="!geoRows.length" class="is-empty">{{ t("ops.screen.noData") }}</div>
            </div>
          </div>
        </ElCol>
      </ElRow>
    </div>

    <div v-show="panel === 'resource'" class="ops-screen__panel">
      <div class="ops-screen__card">
        <div class="ops-screen__label">{{ t("ops.screen.resources") }}</div>
        <ElTable :data="healthRows" size="small" class="ops-screen__table" max-height="520">
          <ElTableColumn prop="service" label="Service" min-width="140" />
          <ElTableColumn prop="version" label="Ver" width="100" />
          <ElTableColumn label="CPU" width="100">
            <template #default="{ row }">{{ row.cpu_cores?.toFixed?.(2) ?? "-" }}</template>
          </ElTableColumn>
          <ElTableColumn label="Mem" width="120">
            <template #default="{ row }">{{ fmtBytes(row.memory_bytes) }}</template>
          </ElTableColumn>
          <ElTableColumn label="Pool" width="100">
            <template #default="{ row }">{{ pct01(row.db_pool_usage) }}</template>
          </ElTableColumn>
          <ElTableColumn label="OK" width="70">
            <template #default="{ row }">
              <span :class="row.health_ok ? 'ok' : 'bad'">{{ row.health_ok ? "●" : "○" }}</span>
            </template>
          </ElTableColumn>
        </ElTable>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { formatDateTime } from "@/utils/format";
import { computed, defineComponent, h, nextTick, onMounted, onUnmounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import { useUserStore } from "@/store/modules/user";
import { DASHBOARD_PATH } from "@/utils/ops/opsRoutes";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import { echarts } from "@/plugins/echarts";
import { bindOpsChartResize } from "@/composables/ops/useOpsChartHost";
import {
  fetchMetricTimeseries,
  fetchMetricsOverview,
  fetchServiceTopology,
  fetchServicesHealth,
} from "@/api/monitoring";
import { listSloStatus } from "@/api/opsReliability";
import { fetchAlertEvents } from "@/api/alerts";
import { postOlapQuery } from "@/api/olap";
import { presetWindow } from "@/composables/ops/useOlapRange";
import { fetchFinanceOverview } from "@/api/billing-ops";
import type { MetricOverview, ServiceHealthRow, TopologyGraph } from "@/types/api/monitoring";
import type { SloStatusRow } from "@/api/opsReliability";
import type { FinanceOverview } from "@/api/billing-ops";
import type { AlertEvent } from "@/types/api/alerts";

defineOptions({ name: "OpsScreen" });
const ScreenStat = defineComponent({
  name: "ScreenStat",
  props: {
    label: { type: String, required: true },
    value: { type: String, required: true },
  },
  setup(props) {
    return () =>
      h("div", { class: "ops-screen__card ops-screen__stat" }, [
        h("div", { class: "ops-screen__label" }, props.label),
        h("div", { class: "ops-screen__value" }, props.value),
      ]);
  },
});

const { t } = useI18n();
const userStore = useUserStore();
const canSeeFinance = computed(() => userStore.canWriteBusiness);
const canSeeTopology = computed(() => userStore.isAdmin);
const panels = ["overview", "business", "resource"] as const;
const router = useRouter();

function exitToDashboard(): void {
  void router.push(DASHBOARD_PATH);
}

const panel = ref<(typeof panels)[number]>("overview");
const refreshInterval = ref(30);
const clock = ref("");
const overview = ref<MetricOverview | null>(null);
const slo = ref<SloStatusRow[]>([]);
const finance = ref<FinanceOverview | null>(null);
const healthRows = ref<ServiceHealthRow[]>([]);
const topProducts = ref<{ name: string; value: number }[]>([]);
const supplierWall = ref<{ name: string; tone: string }[]>([]);
const geoRows = ref<{ dim: string; count: number; width: string }[]>([]);
const alertText = ref("");
const qpsRef = ref<HTMLDivElement>();
const topoRef = ref<HTMLDivElement>();
let qpsChart: echarts.ECharts | null = null;
let topoChart: echarts.ECharts | null = null;
let clockTimer: ReturnType<typeof setInterval> | null = null;
let rotateTimer: ReturnType<typeof setInterval> | null = null;

const burnLabel = computed(() => {
  const hot = slo.value.filter((s) => (s.burn_page ?? 0) > 0 || (s.burn_ticket ?? 0) > 0);
  if (!hot.length) return "OK";
  return hot
    .map((s) => {
      const burn = Number(s.burn_1h ?? s.burn_5m ?? 0);
      return `${s.name}×${Number.isFinite(burn) ? burn.toFixed(1) : "0"}`;
    })
    .join(" · ");
});

const todayRevenue = computed(() => {
  const day = finance.value?.days?.at(-1);
  return day?.revenue ?? finance.value?.revenue_total ?? null;
});
const todayCost = computed(() => {
  if (finance.value && finance.value.cost_status !== "ok") return null;
  const day = finance.value?.days?.at(-1);
  if (day && day.cost_status !== "ok") return null;
  return day?.cost ?? finance.value?.cost_total ?? null;
});
const todayMargin = computed(() => {
  if (finance.value && finance.value.cost_status !== "ok") return null;
  const day = finance.value?.days?.at(-1);
  return day?.margin ?? finance.value?.margin_total ?? null;
});

function fmt(n?: number | null) {
  if (n === null || n === undefined) return "—";
  return new Intl.NumberFormat().format(n);
}
function pct(n?: number | null) {
  if (n === null || n === undefined) return "—";
  return `${n.toFixed(2)}%`;
}
function pct01(n?: number | null) {
  if (n == null) return "-";
  return `${(n * 100).toFixed(0)}%`;
}
function ms(n?: number | null) {
  if (n === null || n === undefined) return "—";
  return `${n.toFixed(1)} ms`;
}
function money(n?: number | string | null) {
  if (n == null || n === "") return "-";
  const v = typeof n === "string" ? Number(n) : n;
  if (!Number.isFinite(v)) return String(n);
  return new Intl.NumberFormat(undefined, { maximumFractionDigits: 2 }).format(v);
}
function fmtBytes(n?: number | null) {
  if (n == null) return "-";
  if (n < 1024) return `${n} B`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KiB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MiB`;
  return `${(n / 1024 ** 3).toFixed(2)} GiB`;
}

async function loadAll() {
  const end = Math.floor(Date.now() / 1000);
  const start = end - 3600;
  const dayStart = end - 86400;
  try {
    overview.value = await fetchMetricsOverview();
  } catch {
    /* keep */
  }
  try {
    const body = await listSloStatus();
    slo.value = body?.status ?? [];
  } catch {
    slo.value = [];
  }
  try {
    healthRows.value = await fetchServicesHealth();
  } catch {
    healthRows.value = [];
  }
  if (canSeeFinance.value) {
    try {
      finance.value = await fetchFinanceOverview(1);
    } catch {
      finance.value = null;
    }
  } else {
    finance.value = null;
  }
  try {
    const events = await fetchAlertEvents({ status: "firing", page: 1, page_size: 20 });
    const items: AlertEvent[] = events?.data ?? [];
    alertText.value = items.length
      ? items.map((e) => `[${e.severity}] ${e.rule_name}`).join("    ·    ")
      : t("ops.screen.noData");
  } catch {
    alertText.value = t("ops.screen.noData");
  }
  const screenWindow = presetWindow("24h");
  try {
    const rank = await postOlapQuery({
      template: "product_rank",
      params: { ...screenWindow, metric: "req_count", limit: 10 },
    });
    const rows = (rank as { rows?: Record<string, unknown>[] }).rows ?? [];
    topProducts.value = rows.map((r) => ({
      name: String(r.product_id ?? r.name ?? "-"),
      value: Number(r.req_count ?? r.value ?? 0),
    }));
  } catch {
    topProducts.value = [];
  }
  try {
    const sla = await postOlapQuery({
      template: "supplier_sla",
      params: { ...screenWindow },
    });
    const rows = (sla as { rows?: Record<string, unknown>[] }).rows ?? [];
    supplierWall.value = rows.slice(0, 24).map((r) => {
      const calls = Number(r.calls ?? 0);
      const failures = Number(r.failures ?? 0);
      const rate = calls > 0 ? failures / calls : 0;
      const tone = rate >= 0.05 ? "red" : rate >= 0.01 ? "yellow" : "green";
      return { name: String(r.supplier_code ?? "-"), tone };
    });
  } catch {
    supplierWall.value = [];
  }
  try {
    const geo = await postOlapQuery({
      template: "security_top",
      params: { ...screenWindow, dim: "country", limit: 10 },
    });
    const rows = (geo as { rows?: Record<string, unknown>[] }).rows ?? [];
    const max = Math.max(1, ...rows.map((r) => Number(r.fail_count ?? r.req_count ?? 0)));
    geoRows.value = rows.map((r) => {
      const count = Number(r.fail_count ?? r.req_count ?? 0);
      return {
        dim: String(r.dim_value ?? r.geo_country ?? "-"),
        count,
        width: `${(100 * count) / max}%`,
      };
    });
  } catch {
    geoRows.value = [];
  }
  await Promise.all([renderQps(dayStart, end), renderTopo(start, end)]);
}

async function renderQps(start: number, end: number) {
  await nextTick();
  if (!qpsRef.value) return;
  if (!qpsChart) {
    qpsChart = echarts.init(qpsRef.value);
    unbindQpsResize?.();
    unbindQpsResize = bindOpsChartResize(qpsRef.value, () => qpsChart);
  }
  const qps = qpsChart;
  if (!qps) return;
  try {
    const series = await fetchMetricTimeseries({
      metric: "qps",
      start,
      end,
      step: 300,
    });
    const points = series.series?.[0]?.points ?? [];
    qps.setOption(
      {
        backgroundColor: "transparent",
        grid: { left: 12, right: 16, top: 28, bottom: 24, containLabel: true },
        xAxis: {
          type: "time",
          axisLabel: { color: "#9fb3cd" },
          axisLine: { lineStyle: { color: "#3a4d6a" } },
        },
        yAxis: {
          type: "value",
          axisLabel: { color: "#9fb3cd" },
          splitLine: { lineStyle: { color: "rgba(120,160,220,0.12)" } },
        },
        series: [
          {
            type: "line",
            showSymbol: false,
            areaStyle: { color: "rgba(64,158,255,0.25)" },
            lineStyle: { color: "#409eff", width: 2 },
            data: points.map((p) => [p.timestamp * (p.timestamp > 1e12 ? 1 : 1000), p.value]),
          },
        ],
      },
      true,
    );
  } catch {
    qps.clear();
  }
}

async function renderTopo(start: number, end: number) {
  if (!canSeeTopology.value) {
    return;
  }
  await nextTick();
  if (!topoRef.value) return;
  if (!topoChart) {
    topoChart = echarts.init(topoRef.value);
    unbindTopoResize?.();
    unbindTopoResize = bindOpsChartResize(topoRef.value, () => topoChart);
  }
  const topo = topoChart;
  if (!topo) return;
  try {
    const data: TopologyGraph = await fetchServiceTopology({ start, end });
    const health: Record<string, string> = {
      green: "#67c23a",
      yellow: "#e6a23c",
      red: "#f56c6c",
      unknown: "#6b7c93",
    };
    topo.setOption(
      {
        series: [
          {
            type: "graph",
            layout: "force",
            roam: false,
            silent: true,
            force: { repulsion: 180, edgeLength: [40, 90] },
            label: { show: true, color: "#d7e3f4", fontSize: 10 },
            data: data.nodes.map((n) => ({
              name: n.id.replace(/^mazbri-/, ""),
              symbolSize: n.id.includes("gateway") ? 28 : 16,
              itemStyle: { color: health[n.health] || health.unknown },
            })),
            links: data.edges.map((e) => ({
              source: e.source.replace(/^mazbri-/, ""),
              target: e.target.replace(/^mazbri-/, ""),
              lineStyle: {
                color: e.error_rate >= 0.05 ? "#f56c6c" : "#5b7ea6",
                width: 1,
              },
            })),
          },
        ],
      },
      true,
    );
  } catch {
    topo.clear();
  }
}

function tickClock() {
  clock.value = formatDateTime(new Date());
}

function rotatePanel() {
  const idx = panels.indexOf(panel.value);
  panel.value = panels[(idx + 1) % panels.length];
}

let unbindQpsResize: (() => void) | null = null;
let unbindTopoResize: (() => void) | null = null;

onMounted(() => {
  tickClock();
  clockTimer = setInterval(tickClock, 1000);
  rotateTimer = setInterval(rotatePanel, 45_000);
  void loadAll();
});

onUnmounted(() => {
  if (clockTimer) clearInterval(clockTimer);
  if (rotateTimer) clearInterval(rotateTimer);
  unbindQpsResize?.();
  unbindTopoResize?.();
  unbindQpsResize = null;
  unbindTopoResize = null;
  qpsChart?.dispose();
  topoChart?.dispose();
});
</script>

<style scoped>
.ops-screen {
  min-height: 100vh;
  padding: 20px 28px 32px;
  color: #e8eef7;
  background: radial-gradient(circle at top, #1a2a44 0%, #0b1220 55%, #070b14 100%);
}
.ops-screen__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 18px;
}
.ops-screen__header h1 {
  margin: 0;
  font-size: 26px;
  letter-spacing: 0.04em;
}
.ops-screen__meta {
  display: flex;
  gap: 16px;
  align-items: center;
  color: #9fb3cd;
}
.ops-screen__tabs {
  display: flex;
  gap: 8px;
}
.ops-screen__tab {
  padding: 6px 12px;
  border: 1px solid rgba(120, 160, 220, 0.25);
  border-radius: 999px;
  color: #9fb3cd;
  background: transparent;
  cursor: pointer;
}
.ops-screen__tab.is-active {
  color: #fff;
  background: rgba(64, 158, 255, 0.25);
  border-color: rgba(64, 158, 255, 0.55);
}
.ops-screen__row {
  margin-top: 16px;
}
.ops-screen__card {
  padding: 18px 20px;
  border: 1px solid rgba(120, 160, 220, 0.25);
  border-radius: 12px;
  background: rgba(16, 28, 48, 0.85);
  box-shadow: inset 0 0 30px rgba(64, 128, 255, 0.08);
  min-height: 96px;
}
.ops-screen__stat .ops-screen__value {
  font-size: 34px;
  font-weight: 700;
}
.ops-screen__label {
  margin-bottom: 10px;
  color: #9fb3cd;
  font-size: 13px;
}
.ops-screen__chart {
  height: 260px;
  overflow: hidden;
}
.ops-screen__chart--sm {
  height: 260px;
  overflow: hidden;
}
.ops-screen__marquee {
  overflow: hidden;
}
.ops-screen__marquee-track {
  white-space: nowrap;
  animation: marquee 28s linear infinite;
  font-size: 16px;
}
@keyframes marquee {
  from { transform: translateX(0); }
  to { transform: translateX(-50%); }
}
.ops-screen__list {
  list-style: none;
  margin: 0;
  padding: 0;
  max-height: 320px;
  overflow: auto;
}
.ops-screen__list li {
  display: flex;
  justify-content: space-between;
  padding: 8px 0;
  border-bottom: 1px solid rgba(120, 160, 220, 0.12);
}
.ops-screen__sla {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(96px, 1fr));
  gap: 8px;
}
.ops-screen__sla-cell {
  padding: 10px 8px;
  border-radius: 8px;
  text-align: center;
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.ops-screen__sla-cell.green { background: rgba(103, 194, 58, 0.25); }
.ops-screen__sla-cell.yellow { background: rgba(230, 162, 60, 0.28); }
.ops-screen__sla-cell.red { background: rgba(245, 108, 108, 0.3); }
.ops-screen__geo-row {
  display: grid;
  grid-template-columns: 80px 1fr 64px;
  gap: 10px;
  align-items: center;
  margin-bottom: 8px;
}
.ops-screen__bar {
  height: 10px;
  border-radius: 999px;
  background: rgba(120, 160, 220, 0.15);
  overflow: hidden;
}
.ops-screen__bar i {
  display: block;
  height: 100%;
  background: linear-gradient(90deg, #409eff, #67c23a);
}
.ops-screen__table {
  --el-table-bg-color: transparent;
  --el-table-tr-bg-color: transparent;
  --el-table-header-bg-color: rgba(16, 28, 48, 0.9);
  --el-table-text-color: #e8eef7;
  --el-table-header-text-color: #9fb3cd;
  --el-table-border-color: rgba(120, 160, 220, 0.15);
  background: transparent;
}
.ok { color: #67c23a; }
.bad { color: #f56c6c; }
.is-empty { color: #6b7c93; padding: 12px 0; }

.ops-screen__exit {
  margin-left: 8px;
  padding: 4px 12px;
  border: 1px solid rgba(255, 255, 255, 0.28);
  border-radius: 6px;
  background: transparent;
  color: inherit;
  cursor: pointer;
  font: inherit;
}
.ops-screen__exit:hover {
  border-color: rgba(255, 255, 255, 0.55);
  background: rgba(255, 255, 255, 0.08);
}
</style>
