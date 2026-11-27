import React, { useEffect, useMemo, useRef, useState } from "react";
import {
  ChartCard,
  MetricSeriesChart,
  isMetricSeriesEmpty,
} from "@/components/charts";
import { fetchMetricTimeseries } from "@/api/monitoring";
import { useT } from "@/locales";
import type { MetricSeries } from "@/types/api/monitoring";
import { withContainerDisplayName } from "@/utils/ops/container-name";
import { getInfraCharts, type InfraChartTab } from "@/utils/ops/infra-charts";
import type { TimeRangeValue } from "@/utils/ops/time-range";
import { resolveStep } from "../shared/opsHelpers";

export interface InfraMetricsPanelProps {
  tab: InfraChartTab;
  noteKey: string;
  timeRange: TimeRangeValue;
  reloadTick: number;
}

export function InfraMetricsPanel({
  tab,
  noteKey,
  timeRange,
  reloadTick,
}: InfraMetricsPanelProps) {
  const { t } = useT();
  const charts = useMemo(() => getInfraCharts(tab), [tab]);
  const [loading, setLoading] = useState(false);
  const [seriesMap, setSeriesMap] = useState<
    Record<string, MetricSeries | null>
  >({});
  const seqRef = useRef(0);

  const chartWindow = useMemo(
    () => ({ start: timeRange.start, end: timeRange.end }),
    [timeRange.start, timeRange.end],
  );

  useEffect(() => {
    const mine = ++seqRef.current;
    const ctrl = new AbortController();
    const step = resolveStep(Math.max(60, timeRange.end - timeRange.start));
    setLoading(true);

    Promise.all(
      charts.map(async (def) => {
        try {
          const data = await fetchMetricTimeseries(
            {
              metric: def.metric,
              start: timeRange.start,
              end: timeRange.end,
              step,
            },
            ctrl.signal,
          );
          if (!data) return [def.metric, null] as const;
          if (def.remapContainers && data.series?.length) {
            return [
              def.metric,
              { ...data, series: withContainerDisplayName(data.series) },
            ] as const;
          }
          return [def.metric, data] as const;
        } catch {
          return [def.metric, null] as const;
        }
      }),
    )
      .then((entries) => {
        if (mine !== seqRef.current || ctrl.signal.aborted) return;
        setSeriesMap(Object.fromEntries(entries));
      })
      .finally(() => {
        if (mine === seqRef.current) setLoading(false);
      });

    return () => ctrl.abort();
  }, [charts, timeRange.start, timeRange.end, reloadTick]);

  return (
    <div className="space-y-3 min-w-0">
      <div className="text-xs text-muted-foreground">{t(noteKey)}</div>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        {charts.map((chart) => {
          const data = seriesMap[chart.metric] ?? null;
          const empty = !loading && isMetricSeriesEmpty(data);
          return (
            <ChartCard
              key={chart.metric}
              title={t(chart.title)}
              loading={loading}
              empty={empty}
              emptyText={t("ops.charts.empty")}
              height={220}
            >
              <MetricSeriesChart
                series={data}
                window={chartWindow}
                height={220}
              />
            </ChartCard>
          );
        })}
      </div>
    </div>
  );
}
