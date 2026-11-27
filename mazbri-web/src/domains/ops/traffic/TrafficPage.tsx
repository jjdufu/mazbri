import React, { useMemo, useState } from "react";
import { Tabs, type TabOption } from "@/components/ui";
import { useT } from "@/locales";
import {
  MERGED_DEFAULT_TAB,
  getMergedRouteTabs,
  type MergedTabId,
} from "@/utils/ops/merged-route-tabs";
import { OpsTimeBar } from "../shared/OpsTimeBar";
import { useOpsTabUrl, useOpsTimeRangeUrl } from "../shared/opsHelpers";
import { CallMonitorPanel } from "./CallMonitorPanel";
import { TrafficTracesPanel } from "./TrafficTracesPanel";

type TrafficTabId = MergedTabId<"traffic">;

const LABEL_MAP: Record<TrafficTabId, string> = {
  customer: "ops.calls.tabs.customer",
  upstream: "ops.calls.tabs.upstream",
  traces: "ops.calls.tabs.traces",
};

export function TrafficPage() {
  const { t } = useT();
  const { timeRange, setTimeRange } = useOpsTimeRangeUrl("1h");
  const [refreshInterval, setRefreshInterval] = useState(0);
  const [reloadTick, setReloadTick] = useState(0);

  const tabDefs = useMemo(
    () =>
      getMergedRouteTabs("traffic").map((seed) => ({
        ...seed,
        label: t(LABEL_MAP[seed.id]),
      })),
    [t],
  );

  const { visibleTabs, activeTab, setActiveTab } = useOpsTabUrl<TrafficTabId>(
    tabDefs,
    MERGED_DEFAULT_TAB.traffic,
  );

  const tabOptions = useMemo<TabOption<TrafficTabId>[]>(
    () =>
      visibleTabs.map((item) => ({
        key: item.id,
        label: item.label ?? item.id,
      })),
    [visibleTabs],
  );

  return (
    <div className="space-y-4 min-w-0">
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border pb-2">
        {tabOptions.length > 1 ? (
          <Tabs
            items={tabOptions}
            value={activeTab}
            onChange={setActiveTab}
            className="border-b-0 -mb-2"
          />
        ) : (
          <span />
        )}
        <OpsTimeBar
          value={timeRange}
          onChange={setTimeRange}
          refreshInterval={refreshInterval}
          onRefreshIntervalChange={setRefreshInterval}
          onRefresh={() => setReloadTick((v) => v + 1)}
        />
      </div>

      {activeTab === "customer" && (
        <CallMonitorPanel
          mode="customer"
          timeRange={timeRange}
          onTimeRangeChange={setTimeRange}
          reloadTick={reloadTick}
        />
      )}

      {activeTab === "upstream" && (
        <CallMonitorPanel
          mode="upstream"
          timeRange={timeRange}
          onTimeRangeChange={setTimeRange}
          reloadTick={reloadTick}
        />
      )}

      {activeTab === "traces" && (
        <TrafficTracesPanel timeRange={timeRange} reloadTick={reloadTick} />
      )}
    </div>
  );
}
