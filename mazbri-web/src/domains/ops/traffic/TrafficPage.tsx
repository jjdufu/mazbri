import React, { useMemo, useState } from "react";
import { PageHeaderActions } from "@/components/layout";
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
    <div className="flex flex-col flex-1 min-h-0 gap-4 min-w-0">
      <PageHeaderActions>
        <div className="flex flex-wrap items-center justify-end gap-2.5">
          <OpsTimeBar
            value={timeRange}
            onChange={setTimeRange}
            refreshInterval={refreshInterval}
            onRefreshIntervalChange={setRefreshInterval}
            onRefresh={() => setReloadTick((v) => v + 1)}
          />
          {tabOptions.length > 1 && (
            <>
              <span
                className="hidden sm:inline-block h-4 w-px bg-border-subtle"
                aria-hidden="true"
              />
              <Tabs
                variant="inline"
                items={tabOptions}
                value={activeTab}
                onChange={setActiveTab}
              />
            </>
          )}
        </div>
      </PageHeaderActions>

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
