<template>
  <div class="ops-traffic art-full-height">
    <div class="ops-traffic__toolbar">
      <h2>{{ t("menus.mazbri.trafficErrors") }}</h2>
      <div class="ops-traffic__actions">
        <TimeRangePicker v-model="timeRange" @change="reload" />
        <AutoRefresh v-model="refreshInterval" @tick="reload" />
      </div>
    </div>

    <ElRow :gutter="12">
      <ElCol :xs="24" :lg="14">
        <div class="ops-traffic__card">
          <h3>{{ t("ops.traffic.errorMatrix") }}</h3>
          <HeatmapChart
            :cells="matrixCells"
            :x-labels="matrixX"
            :y-labels="matrixY"
            :loading="loading"
            height="320px"
          />
          <ArtTable
            :data="matrixRows"
            :loading="loading"
            :columns="matrixColumns"
            class="surface-panel-table"
            stripe
            @row-click="onErrorClick"
          />
        </div>
      </ElCol>
      <ElCol :xs="24" :lg="10">
        <div class="ops-traffic__card">
          <h3>{{ t("ops.traffic.errorAttribution") }}</h3>
          <PieChart :slices="attrSlices" :loading="loading" height="260px" />
        </div>
        <div class="ops-traffic__card">
          <h3>{{ t("ops.traffic.responseFlags") }}</h3>
          <CategoryBarChart
            horizontal
            :categories="flagCategories"
            :series="flagSeries"
            :loading="loading"
            height="220px"
          />
        </div>
      </ElCol>
    </ElRow>

    <div class="ops-traffic__card">
      <h3>{{ t("ops.traffic.newErrorCodes") }}</h3>
      <ArtTable
        :data="newErrors"
        :loading="loading"
        :columns="newColumns"
        class="surface-panel-table"
        stripe
        @row-click="onErrorClick"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import TimeRangePicker, {
  type TimeRangeValue,
} from "@/components/ops/TimeRangePicker.vue";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import CategoryBarChart from "@/components/ops/CategoryBarChart.vue";
import HeatmapChart, { type HeatCell } from "@/components/ops/HeatmapChart.vue";
import PieChart from "@/components/ops/PieChart.vue";
import { toOlapRange } from "@/composables/ops/useOlapRange";
import { postOlapQuery } from "@/api/olap";
import { buildCallsRoute } from "@/utils/ops/calls-link";
import type {
  AttributionRow,
  ErrorCellRow,
  FlagRow,
  NewErrorRow,
} from "@/types/api/olap";
import type { ColumnOption } from "@/types/component";

defineOptions({ name: "OpsTrafficErrors" });

const { t } = useI18n();

const timeRange = ref<TimeRangeValue>({
  preset: "6h",
  start: Math.floor(Date.now() / 1000) - 6 * 3600,
  end: Math.floor(Date.now() / 1000),
});
const refreshInterval = ref(30);
const loading = ref(false);
const matrixRows = ref<ErrorCellRow[]>([]);
const attribution = ref<AttributionRow[]>([]);
const flags = ref<FlagRow[]>([]);
const newErrors = ref<NewErrorRow[]>([]);

const matrixX = computed(() => [
  ...new Set(matrixRows.value.map((r) => r.error_code || "—")),
]);
const matrixY = computed(() => [
  ...new Set(matrixRows.value.map((r) => r.error_stage || "—")),
]);
const matrixCells = computed<HeatCell[]>(() =>
  matrixRows.value.map((r) => ({
    x: r.error_code || "—",
    y: r.error_stage || "—",
    value: r.count,
  })),
);
const matrixColumns = computed<ColumnOption<ErrorCellRow>[]>(() => [
  { prop: "error_code", label: "error_code" },
  { prop: "error_stage", label: "error_stage" },
  { prop: "count", label: t("ops.traffic.columns.count") },
]);

const attrSlices = computed(() =>
  attribution.value.map((r) => ({
    name: r.error_class || "unknown",
    value: r.count,
  })),
);
const flagCategories = computed(() =>
  flags.value.map((r) => r.response_flags || "—"),
);
const flagSeries = computed(() => [
  {
    name: t("ops.traffic.columns.count"),
    data: flags.value.map((r) => r.count),
  },
]);
const newColumns = computed<ColumnOption<NewErrorRow>[]>(() => [
  { prop: "error_code", label: "error_code" },
  { prop: "count", label: t("ops.traffic.columns.count") },
]);

const router = useRouter();

function onErrorClick(row: ErrorCellRow | NewErrorRow) {
  if (!row.error_code) return;
  void router.push(
    buildCallsRoute({
      tab: "customer",
      start: timeRange.value.start,
      end: timeRange.value.end,
      filters: { error_code: row.error_code },
    }),
  );
}

async function reload() {
  loading.value = true;
  const range = toOlapRange(timeRange.value, true);
  try {
    const [matrix, attr, flag, neu] = await Promise.all([
      postOlapQuery<ErrorCellRow>({
        template: "error_matrix",
        params: { range },
      }),
      postOlapQuery<AttributionRow>({
        template: "error_attribution",
        params: { range },
      }),
      postOlapQuery<FlagRow>({
        template: "response_flags",
        params: { range },
      }),
      postOlapQuery<NewErrorRow>({
        template: "new_error_codes",
        params: { range },
      }),
    ]);
    matrixRows.value = matrix.rows ?? [];
    attribution.value = attr.rows ?? [];
    flags.value = flag.rows ?? [];
    newErrors.value = neu.rows ?? [];
  } catch {
    ElMessage.error(t("ops.traffic.loadFailed"));
  } finally {
    loading.value = false;
  }
}

onMounted(reload);
</script>

<style scoped>
.ops-traffic__toolbar {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 12px;
}
.ops-traffic__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}
.ops-traffic__card {
  margin-bottom: 12px;
  padding: 12px 16px;
  border-radius: 8px;
  background: var(--el-bg-color);
  border: 1px solid var(--el-border-color-lighter);
}
.ops-traffic__card h3 {
  margin: 0 0 8px;
  font-size: 14px;
  font-weight: 600;
}
</style>
