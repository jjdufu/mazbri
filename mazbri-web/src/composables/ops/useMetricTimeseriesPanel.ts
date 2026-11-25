import { isRef, onMounted, ref, unref, watch, type Ref } from "vue";
import { ElMessage } from "element-plus";
import { fetchMetricTimeseries } from "@/api/monitoring";
import type { MetricSeries } from "@/types/api/monitoring";
import { resolveStep } from "@/composables/ops/useStep";
import { useLatestRequest } from "@/composables/useLatestRequest";
import { withContainerDisplayName } from "@/utils/ops/container-name";
import type { TimeRangeValue } from "@/utils/ops/time-range";

export interface MetricChartSpec {
  metric: string;
  remapContainers?: boolean;
}

/**
 * 多指标时序面板加载器：供基础设施等图表 Tab 复用，禁止各页复制 Promise.all 样板。
 */
export function useMetricTimeseriesPanel(options: {
  charts: Ref<readonly MetricChartSpec[]> | readonly MetricChartSpec[];
  timeRange: Ref<TimeRangeValue>;
  reloadTick?: Ref<number>;
}) {
  const series = ref<Record<string, MetricSeries>>({});

  function chartList(): readonly MetricChartSpec[] {
    return unref(options.charts);
  }

  const { loading, run } = useLatestRequest(async (ctx) => {
    const { start, end } = options.timeRange.value;
    const step = resolveStep(end - start);
    const next: Record<string, MetricSeries> = {};
    await Promise.all(
      chartList().map(async (chart) => {
        const res = await fetchMetricTimeseries(
          {
            metric: chart.metric,
            start,
            end,
            step,
          },
          ctx.signal,
        );
        if (chart.remapContainers) {
          next[chart.metric] = {
            ...res,
            series: withContainerDisplayName(res.series || []),
          };
        } else {
          next[chart.metric] = res;
        }
      }),
    );
    if (!ctx.isLatest()) return;
    series.value = next;
  });

  async function reload() {
    try {
      await run();
    } catch (e) {
      ElMessage.error((e as Error).message || String(e));
    }
  }

  onMounted(() => {
    void reload();
  });

  watch(
    () => {
      const range = options.timeRange.value;
      return [
        range.start,
        range.end,
        range.preset,
        options.reloadTick?.value ?? 0,
        isRef(options.charts)
          ? options.charts.value.length
          : options.charts.length,
      ] as const;
    },
    () => {
      void reload();
    },
  );

  return { loading, series, reload };
}
