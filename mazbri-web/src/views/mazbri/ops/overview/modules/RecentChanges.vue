<template>
  <OpsCard class="recent-changes">
    <template #title>
      {{ t("ops.overview.recentChanges") }}
    </template>
    <template #extra>
      <ElButton link type="primary" @click="openCharts">
        {{ t("ops.overview.changeCharts") }}
      </ElButton>
    </template>

    <EmptyState
      v-if="loaded && !loading && rows.length === 0"
      compact
      :image-size="56"
      :description="t('ops.overview.changeEmpty')"
    />
    <OpsTableWrap v-else>
      <ArtTable
        :data="rows"
        :loading="loading"
        :columns="columns"
        stripe
        class="recent-changes__table"
        @row-click="onRowClick"
      />
    </OpsTableWrap>

    <AdaptiveDrawer
      v-model="detailOpen"
      :title="t('ops.overview.changeDetailTitle')"
      destroy-on-close
      size="420px"
    >
      <ElDescriptions
        v-if="selected"
        :column="1"
        border
        size="small"
        class="recent-changes__desc"
      >
        <ElDescriptionsItem
          v-for="field in drawerFields"
          :key="field.prop"
          :label="field.label"
        >
          <pre
            v-if="field.prop === 'detail'"
            class="recent-changes__detail"
          >{{ changeFieldDisplay(selected, field.prop) }}</pre>
          <span v-else>{{ changeFieldDisplay(selected, field.prop) }}</span>
        </ElDescriptionsItem>
      </ElDescriptions>
    </AdaptiveDrawer>

    <AdaptiveDrawer
      v-model="chartsOpen"
      :title="t('ops.overview.changeCharts')"
      destroy-on-close
      size="640px"
    >
      <div class="recent-changes__charts">
        <OpsCard :title="t('ops.assets.routeReload')">
          <MultiSeriesChart
            :series="routeSeries"
            :loading="chartsLoading"
            :annotations="annotations"
            :window="lastWindow"
            height="200px"
          />
        </OpsCard>
        <OpsCard :title="t('ops.assets.connectorPublish')">
          <MultiSeriesChart
            :series="publishSeries"
            :loading="chartsLoading"
            :annotations="annotations"
            :window="lastWindow"
            height="200px"
          />
        </OpsCard>
      </div>
    </AdaptiveDrawer>
  </OpsCard>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import AdaptiveDrawer from "@/components/common/AdaptiveDrawer.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import EmptyState from "@/components/common/empty-state/index.vue";
import OpsTableWrap from "@/components/ops/OpsTableWrap.vue";
import MultiSeriesChart from "@/components/ops/MultiSeriesChart.vue";
import { resolveStep } from "@/composables/ops/useStep";
import { fetchMetricTimeseries } from "@/api/monitoring";
import { fetchOpsChanges, type ChangeEvent } from "@/api/opsChanges";
import type { MetricSeries } from "@/types/api/monitoring";
import {
  buildChangeAnnotations,
  buildChangeDrawerFields,
  buildChangeTableColumns,
  changeFieldDisplay,
  normalizeChangeEvents,
  takeChangeTop,
} from "@/utils/ops/changes";

defineOptions({ name: "OpsOverviewRecentChanges" });

const LIST_LIMIT = 20;

const { t } = useI18n();
const loading = ref(false);
const chartsLoading = ref(false);
const rows = ref<ChangeEvent[]>([]);
const selected = ref<ChangeEvent | null>(null);
const detailOpen = ref(false);
const chartsOpen = ref(false);
const routeSeries = ref<MetricSeries | null>(null);
const publishSeries = ref<MetricSeries | null>(null);
const annotations = computed(() => buildChangeAnnotations(rows.value));

const columns = computed(() => buildChangeTableColumns(t));
const drawerFields = computed(() => buildChangeDrawerFields(t));

function onRowClick(row: ChangeEvent) {
  selected.value = row;
  detailOpen.value = true;
}

async function loadCharts(start: number, end: number) {
  chartsLoading.value = true;
  try {
    const step = resolveStep(end - start);
    const [route, publish] = await Promise.all([
      fetchMetricTimeseries({
        metric: "route_reload_total",
        start,
        end,
        step,
      }).catch(() => null),
      fetchMetricTimeseries({
        metric: "connector_publish_total",
        start,
        end,
        step,
      }).catch(() => null),
    ]);
    routeSeries.value = route;
    publishSeries.value = publish;
  } finally {
    chartsLoading.value = false;
  }
}

const lastWindow = ref<{ start: number; end: number } | null>(null);
const loaded = ref(false);
let seq = 0;

async function openCharts() {
  chartsOpen.value = true;
  const w = lastWindow.value;
  if (w) await loadCharts(w.start, w.end);
}

/** 按全局时间窗取变更与审计事件；速率图沿用同一窗口。 */
async function reload(window: { start: number; end: number }) {
  const mine = ++seq;
  lastWindow.value = window;
  loading.value = true;
  try {
    const raw = await fetchOpsChanges({ ...window, limit: LIST_LIMIT });
    if (mine !== seq) return;
    rows.value = takeChangeTop(normalizeChangeEvents(raw?.events), LIST_LIMIT);
    if (chartsOpen.value) void loadCharts(window.start, window.end);
  } catch (error) {
    if (mine !== seq) return;
    rows.value = [];
    ElMessage.error(
      error instanceof Error
        ? error.message
        : t("ops.overview.changeLoadFailed"),
    );
  } finally {
    if (mine === seq) {
      loading.value = false;
      loaded.value = true;
    }
  }
}

defineExpose({ reload });
</script>

<style scoped>
.recent-changes__table {
  cursor: pointer;
}
.recent-changes__desc {
  width: 100%;
}
.recent-changes__detail {
  margin: 0;
  white-space: pre-wrap;
  word-break: break-word;
  font-family: var(--el-font-family);
  font-size: 12px;
  line-height: 1.45;
}
.recent-changes__charts {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
</style>
