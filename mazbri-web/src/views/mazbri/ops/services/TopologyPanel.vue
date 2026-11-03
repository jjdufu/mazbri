<template>
  <div class="ops-services-topology-panel">
    <OpsPanelToolbar>
      <TimeRangePicker v-model="timeRange" @change="bumpReload" />
    </OpsPanelToolbar>
    <p class="ops-services-topology-panel__hint">
      {{ t("ops.services.topologyPage.hint") }}
    </p>
    <OpsCard>
      <div
        ref="graphRef"
        v-loading="loading"
        class="ops-services-topology-panel__graph"
      />
    </OpsCard>
  </div>
</template>

<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import { ElMessage } from "element-plus";
import OpsCard from "@/components/ops/OpsCard.vue";
import OpsPanelToolbar from "@/components/ops/OpsPanelToolbar.vue";
import TimeRangePicker from "@/components/ops/TimeRangePicker.vue";
import { bindOpsChartResize } from "@/composables/ops/useOpsChartHost";
import { useOpsSharedRange } from "@/composables/ops/useOpsSharedRange";
import { echarts, type EChartsOption } from "@/plugins/echarts";
import { fetchServiceTopology } from "@/api/monitoring";
import type { TopologyEdge, TopologyGraph } from "@/types/api/monitoring";
import { buildTraceSearchRoute } from "@/utils/ops/opsRoutes";

defineOptions({ name: "OpsServicesTopologyPanel" });

const { t } = useI18n();
const router = useRouter();
const { timeRange, reloadTick, bumpReload } = useOpsSharedRange();
const graphRef = ref<HTMLDivElement>();
const loading = ref(false);
let chart: echarts.ECharts | null = null;
let seq = 0;
let graph: TopologyGraph | null = null;
let unbindResize: (() => void) | null = null;

const HEALTH: Record<string, string> = {
  green: "#67c23a",
  yellow: "#e6a23c",
  red: "#f56c6c",
  unknown: "#909399",
};
const KIND: Record<string, string> = {
  rpc: "#409eff",
  storage: "#e6a23c",
  upstream: "#67c23a",
};

function categoryOf(kind: string): number {
  if (kind === "middleware") return 1;
  if (kind === "upstream" || kind === "client") return 2;
  return 0;
}

function edgeColor(e: TopologyEdge): string {
  if (e.error_rate >= 0.05) return "#f56c6c";
  if (e.error_rate >= 0.01) return "#e6a23c";
  return KIND[e.kind] || "#409eff";
}

function render() {
  if (!graphRef.value || !graph) return;
  if (!chart) {
    chart = echarts.init(graphRef.value);
    unbindResize?.();
    unbindResize = bindOpsChartResize(graphRef.value, () => chart);
  }
  const active = chart;
  if (!active) return;
  const data = graph;
  const maxQps = Math.max(1, ...data.edges.map((e) => e.qps));
  const categories = [
    { name: t("ops.traces.topology.kinds.rpc") },
    { name: t("ops.traces.topology.kinds.storage") },
    { name: t("ops.traces.topology.kinds.upstream") },
  ];
  const option: EChartsOption = {
    tooltip: {
      formatter: (p) => {
        const item = Array.isArray(p) ? p[0] : p;
        if (item.dataType === "edge") {
          const edge = data.edges[item.dataIndex];
          if (!edge) return "";
          const origin = t(`ops.services.topologyPage.origins.${edge.origin}`);
          return `${edge.source} → ${edge.target}<br/>${t("ops.services.topologyPage.qps")}: ${edge.qps.toFixed(2)}<br/>${t("ops.services.topologyPage.errorRate")}: ${(edge.error_rate * 100).toFixed(2)}%<br/>${t("ops.services.topologyPage.p99")}: ${edge.p99_ms.toFixed(1)} ms<br/>${t("ops.services.topologyPage.origin")}: ${origin}`;
        }
        return String(item.name);
      },
    },
    legend: [{ data: categories.map((c) => c.name), top: 0 }],
    series: [
      {
        type: "graph",
        layout: "force",
        roam: true,
        draggable: true,
        categories,
        force: { repulsion: 480, edgeLength: [80, 220] },
        label: { show: true, position: "right" },
        edgeSymbol: ["none", "arrow"],
        edgeSymbolSize: 8,
        data: data.nodes.map((n) => ({
          name: n.id,
          category: categoryOf(n.kind),
          symbolSize: n.id === "mazbri-gateway" || n.id === "client" ? 44 : 28,
          itemStyle: { color: HEALTH[n.health] || HEALTH.unknown },
        })),
        links: data.edges.map((e) => ({
          source: e.source,
          target: e.target,
          label: {
            show: false,
            formatter: `${e.qps.toFixed(1)} · ${(e.error_rate * 100).toFixed(0)}%`,
            fontSize: 10,
          },
          emphasis: {
            label: { show: true, fontSize: 10 },
          },
          lineStyle: {
            color: edgeColor(e),
            width: 1 + 4 * (e.qps / maxQps),
            curveness: 0.12,
            type: e.origin === "static" ? "dashed" : "solid",
          },
        })),
      },
    ],
  };
  active.setOption(option, true);
  active.off("click");
  active.on("click", (params) => {
    if (params.dataType !== "edge") return;
    const edge = data.edges[params.dataIndex as number];
    if (!edge) return;
    void router.push(
      buildTraceSearchRoute({
        start: data.start,
        end: data.end,
        filters: { service: edge.source },
      }),
    );
  });
}

async function reload() {
  const id = ++seq;
  loading.value = true;
  try {
    const end = Math.floor(Date.now() / 1000);
    const start =
      typeof timeRange.value.start === "number"
        ? timeRange.value.start
        : end - 3600;
    const endTs =
      typeof timeRange.value.end === "number" ? timeRange.value.end : end;
    const res = await fetchServiceTopology({ start, end: endTs });
    if (id !== seq) return;
    graph = res;
    await nextTick();
    render();
  } catch {
    if (id === seq) ElMessage.error(t("ops.services.topologyPage.loadFailed"));
  } finally {
    if (id === seq) loading.value = false;
  }
}

onMounted(() => {
  void reload();
});

watch(reloadTick, () => {
  void reload();
});

onUnmounted(() => {
  unbindResize?.();
  unbindResize = null;
  chart?.dispose();
  chart = null;
});
</script>

<style scoped>
.ops-services-topology-panel__hint {
  margin: 0 0 12px;
  color: var(--el-text-color-secondary);
  font-size: 13px;
}

.ops-services-topology-panel__graph {
  flex: 1;
  min-height: 560px;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 8px;
  background: var(--el-bg-color);
}
</style>
