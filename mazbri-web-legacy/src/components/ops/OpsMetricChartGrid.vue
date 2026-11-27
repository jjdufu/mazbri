<template>
  <div class="ops-metric-chart-grid ops-tab-page-grid">
    <OpsCard v-if="allEmpty" :info="note">
      <EmptyState compact :description="t('ops.charts.noSamplesInRange')" />
    </OpsCard>
    <ElRow v-else :gutter="16">
      <ElCol
        v-for="(chart, idx) in charts"
        :key="chart.metric"
        :xs="24"
        :lg="12"
      >
        <OpsCard
          :title="chartTitle(chart)"
          :info="idx === 0 ? note : undefined"
        >
          <MultiSeriesChart
            :series="series[chart.metric]"
            :loading="loading"
            :window="window"
            height="220px"
          />
        </OpsCard>
      </ElCol>
    </ElRow>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import EmptyState from "@/components/common/empty-state/index.vue";
import MultiSeriesChart from "@/components/ops/MultiSeriesChart.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import type { MetricSeries } from "@/types/api/monitoring";

defineOptions({ name: "OpsMetricChartGrid" });

export interface OpsMetricChartGridItem {
  metric: string;
  /** 已翻译标题，或 i18n key（当 titleIsKey 为 true） */
  title: string;
}

const props = withDefaults(
  defineProps<{
    charts: readonly OpsMetricChartGridItem[];
    series: Record<string, MetricSeries>;
    loading?: boolean;
    note?: string;
    /** title 字段是否为 i18n key */
    titleIsKey?: boolean;
    window?: { start: number; end: number } | null;
  }>(),
  {
    loading: false,
    note: undefined,
    titleIsKey: true,
    window: null,
  },
);

const { t } = useI18n();

const allEmpty = computed(() => {
  if (props.loading || props.charts.length === 0) return false;
  return props.charts.every((chart) => {
    const item = props.series[chart.metric];
    if (!item || !Array.isArray(item.series) || item.series.length === 0) {
      return true;
    }
    return item.series.every((s) => !s.points || s.points.length === 0);
  });
});

function chartTitle(chart: OpsMetricChartGridItem): string {
  return props.titleIsKey ? t(chart.title) : chart.title;
}
</script>

<style scoped lang="scss">
.ops-metric-chart-grid__note {
  margin: 0;
  color: var(--el-text-color-secondary);
  font-size: 13px;
  line-height: 1.5;
}
</style>
