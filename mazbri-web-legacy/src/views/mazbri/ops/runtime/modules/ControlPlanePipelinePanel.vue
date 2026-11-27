<template>
  <ElRow :gutter="16">
    <ElCol :xs="24" :lg="12">
      <OpsCard
        :title="t('ops.assets.routeReload')"
        :info="t('ops.infra.controlPlaneNote')"
      >
        <MultiSeriesChart
          :series="routeSeries"
          :loading="loading"
          :annotations="changeAnnotations"
          :window="chartWindow"
          height="220px"
        />
      </OpsCard>
    </ElCol>
    <ElCol :xs="24" :lg="12">
      <OpsCard :title="t('ops.assets.connectorPublish')">
        <MultiSeriesChart
          :series="publishSeries"
          :loading="loading"
          :annotations="changeAnnotations"
          :window="chartWindow"
          height="220px"
        />
      </OpsCard>
    </ElCol>
  </ElRow>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import MultiSeriesChart from "@/components/ops/MultiSeriesChart.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import { fetchMetricTimeseries } from "@/api/monitoring";
import { fetchOpsChanges, type ChangeEvent } from "@/api/opsChanges";
import type { MetricSeries } from "@/types/api/monitoring";
import { useOpsRole } from "@/composables/ops/useOpsRole";
import { useOpsSharedRange } from "@/composables/ops/useOpsSharedRange";
import { resolveStep } from "@/composables/ops/useStep";
import {
  buildChangeAnnotations,
  normalizeChangeEvents,
  takeChangeTop,
} from "@/utils/ops/changes";

defineOptions({ name: "ControlPlanePipelinePanel" });

const { t } = useI18n();
const { isBiz } = useOpsRole();
const { timeRange, reloadTick } = useOpsSharedRange();

const loading = ref(false);
const routeSeries = ref<MetricSeries | null>(null);
const publishSeries = ref<MetricSeries | null>(null);
const changeEvents = ref<ChangeEvent[]>([]);

const chartWindow = computed(() => ({
  start: timeRange.value.start,
  end: timeRange.value.end,
}));

const changeAnnotations = computed(() =>
  buildChangeAnnotations(changeEvents.value),
);

let seq = 0;

async function load() {
  if (!isBiz.value) return;
  const current = ++seq;
  loading.value = true;
  const start = timeRange.value.start;
  const end = timeRange.value.end;
  const step = resolveStep(Math.max(60, end - start));
  try {
    const [changesRes, route, publish] = await Promise.all([
      fetchOpsChanges({ start, end, limit: 50 }).catch(() => null),
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
    if (current !== seq) return;
    changeEvents.value = takeChangeTop(
      normalizeChangeEvents(changesRes?.events),
      50,
    );
    routeSeries.value = route;
    publishSeries.value = publish;
  } finally {
    if (current === seq) {
      loading.value = false;
    }
  }
}

onMounted(() => {
  void load();
});

watch(
  [() => timeRange.value.start, () => timeRange.value.end, reloadTick],
  () => {
    void load();
  },
);
</script>
