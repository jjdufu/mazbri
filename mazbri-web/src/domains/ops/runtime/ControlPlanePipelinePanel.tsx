import React, { useEffect, useMemo, useRef, useState } from "react";
import {
  ChartCard,
  MetricSeriesChart,
  isMetricSeriesEmpty,
} from "@/components/charts";
import { fetchMetricTimeseries } from "@/api/monitoring";
import { fetchOpsChanges, type ChangeEvent } from "@/api/opsChanges";
import { useT } from "@/locales";
import type { MetricSeries } from "@/types/api/monitoring";
import {
  buildChangeAnnotations,
  normalizeChangeEvents,
  takeChangeTop,
} from "@/utils/ops/changes";
import type { TimeRangeValue } from "@/utils/ops/time-range";
import { resolveStep, useOpsRole } from "../shared/opsHelpers";

export interface ControlPlanePipelinePanelProps {
  timeRange: TimeRangeValue;
  reloadTick: number;
}

export function ControlPlanePipelinePanel({
  timeRange,
  reloadTick,
}: ControlPlanePipelinePanelProps) {
  const { t } = useT();
  const { isBiz } = useOpsRole();
  const [loading, setLoading] = useState(false);
  const [routeSeries, setRouteSeries] = useState<MetricSeries | null>(null);
  const [publishSeries, setPublishSeries] = useState<MetricSeries | null>(null);
  const [changeEvents, setChangeEvents] = useState<ChangeEvent[]>([]);
  const seqRef = useRef(0);

  const chartWindow = useMemo(
    () => ({ start: timeRange.start, end: timeRange.end }),
    [timeRange.start, timeRange.end],
  );

  const changeAnnotations = useMemo(
    () => buildChangeAnnotations(changeEvents),
    [changeEvents],
  );

  useEffect(() => {
    if (!isBiz) return;
    const mine = ++seqRef.current;
    const ctrl = new AbortController();
    const start = timeRange.start;
    const end = timeRange.end;
    const step = resolveStep(Math.max(60, end - start));

    setLoading(true);
    Promise.all([
      fetchOpsChanges({ start, end, limit: 50 }, ctrl.signal).catch(() => null),
      fetchMetricTimeseries(
        { metric: "route_reload_total", start, end, step },
        ctrl.signal,
      ).catch(() => null),
      fetchMetricTimeseries(
        { metric: "connector_publish_total", start, end, step },
        ctrl.signal,
      ).catch(() => null),
    ])
      .then(([changesRes, route, publish]) => {
        if (mine !== seqRef.current || ctrl.signal.aborted) return;
        setChangeEvents(
          takeChangeTop(normalizeChangeEvents(changesRes?.events), 50),
        );
        setRouteSeries(route);
        setPublishSeries(publish);
      })
      .finally(() => {
        if (mine === seqRef.current) setLoading(false);
      });

    return () => ctrl.abort();
  }, [isBiz, timeRange.start, timeRange.end, reloadTick]);

  return (
    <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
      <ChartCard
        title={t("ops.assets.routeReload")}
        info={t("ops.infra.controlPlaneNote")}
        loading={loading}
        empty={!loading && isMetricSeriesEmpty(routeSeries)}
        height={220}
      >
        <MetricSeriesChart
          series={routeSeries}
          annotations={changeAnnotations}
          window={chartWindow}
          height={220}
        />
      </ChartCard>

      <ChartCard
        title={t("ops.assets.connectorPublish")}
        loading={loading}
        empty={!loading && isMetricSeriesEmpty(publishSeries)}
        height={220}
      >
        <MetricSeriesChart
          series={publishSeries}
          annotations={changeAnnotations}
          window={chartWindow}
          height={220}
        />
      </ChartCard>
    </div>
  );
}
