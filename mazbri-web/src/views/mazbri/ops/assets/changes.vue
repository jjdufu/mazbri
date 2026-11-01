<template>
  <div class="ops-traffic art-full-height">
    <div class="ops-traffic__toolbar">
      <h2>{{ t("menus.mazbri.assetsChanges") }}</h2>
      <div class="ops-traffic__actions">
        <TimeRangePicker v-model="timeRange" @change="reload" />
        <AutoRefresh v-model="refreshInterval" @tick="reload" />
        <ElSwitch v-model="showChanges" :active-text="t('ops.assets.showChanges')" />
      </div>
    </div>

    <ElRow :gutter="12">
      <ElCol :xs="24" :lg="12">
        <div class="ops-traffic__card">
          <h3>{{ t("ops.assets.routeReload") }}</h3>
          <MultiSeriesChart
            :series="routeSeries"
            :loading="loading"
            :annotations="showChanges ? annotations : []"
            height="220px"
          />
        </div>
      </ElCol>
      <ElCol :xs="24" :lg="12">
        <div class="ops-traffic__card">
          <h3>{{ t("ops.assets.connectorPublish") }}</h3>
          <MultiSeriesChart
            :series="publishSeries"
            :loading="loading"
            :annotations="showChanges ? annotations : []"
            height="220px"
          />
        </div>
      </ElCol>
    </ElRow>

    <div class="ops-traffic__card">
      <h3>{{ t("ops.assets.timeline") }}</h3>
      <ArtTable
        :data="events"
        :loading="loading"
        :columns="columns"
        class="surface-panel-table"
        stripe
      />
    </div>
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
import { fetchOpsChanges, type ChangeEvent } from "@/api/opsChanges";
import type { MetricSeries } from "@/types/api/monitoring";
import type { ColumnOption } from "@/types/component";

defineOptions({ name: "OpsAssetsChanges" });

const { t } = useI18n();
const { resolveStep } = useStep();
const timeRange = ref<TimeRangeValue>({
  preset: "7d",
  start: Math.floor(Date.now() / 1000) - 7 * 86400,
  end: Math.floor(Date.now() / 1000),
});
const refreshInterval = ref(60);
const loading = ref(false);
const showChanges = ref(true);
const events = ref<ChangeEvent[]>([]);
const annotations = ref<ChartAnnotation[]>([]);
const routeSeries = ref<MetricSeries | null>(null);
const publishSeries = ref<MetricSeries | null>(null);

const columns = computed<ColumnOption[]>(() => [
  { prop: "ts", label: t("ops.assets.colTime"), width: 180 },
  { prop: "kind", label: t("ops.assets.colKind"), width: 100 },
  { prop: "title", label: t("ops.assets.colTitle") },
  { prop: "actor", label: t("ops.assets.colActor"), width: 120 },
  { prop: "resource_type", label: t("ops.assets.colResource"), width: 120 },
  { prop: "service", label: t("ops.assets.colService"), width: 100 },
  { prop: "status", label: t("ops.assets.colStatus"), width: 90 },
]);

async function reload() {
  loading.value = true;
  try {
    const step = resolveStep(timeRange.value.end - timeRange.value.start);
    const [route, publish, changes] = await Promise.all([
      fetchMetricTimeseries({
        metric: "route_reload_total",
        start: timeRange.value.start,
        end: timeRange.value.end,
        step,
      }).catch(() => null),
      fetchMetricTimeseries({
        metric: "connector_publish_total",
        start: timeRange.value.start,
        end: timeRange.value.end,
        step,
      }).catch(() => null),
      fetchOpsChanges({
        start: timeRange.value.start,
        end: timeRange.value.end,
        limit: 100,
      }).catch(() => ({ events: [] })),
    ]);
    routeSeries.value = route;
    publishSeries.value = publish;
    events.value = changes?.events || [];
    annotations.value = events.value.map((e) => ({
      timestamp: Math.floor(new Date(e.ts).getTime() / 1000),
      label: e.kind,
      color:
        e.kind === "route"
          ? "#409eff"
          : e.kind === "publish"
            ? "#67c23a"
            : "#e6a23c",
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
  flex-wrap: wrap;
  gap: 8px;
}
.ops-traffic__actions {
  display: flex;
  gap: 8px;
  align-items: center;
  flex-wrap: wrap;
}
.ops-traffic__card {
  background: var(--el-bg-color);
  border-radius: 8px;
  padding: 12px;
  margin-bottom: 12px;
}
</style>
