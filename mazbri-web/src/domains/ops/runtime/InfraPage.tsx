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
import { HealthPanel } from "./HealthPanel";
import { InfraMetricsPanel } from "./InfraMetricsPanel";

type InfraTabId = MergedTabId<"infra">;

const LABEL_MAP: Record<InfraTabId, string> = {
  middleware: "menus.mazbri.infraMiddleware",
  hosts: "menus.mazbri.infraHosts",
};

export function InfraPage() {
  const { t } = useT();
  const { timeRange, setTimeRange } = useOpsTimeRangeUrl("1h");
  const [refreshInterval, setRefreshInterval] = useState(30);
  const [reloadTick, setReloadTick] = useState(0);

  const tabDefs = useMemo(
    () =>
      getMergedRouteTabs("infra").map((seed) => ({
        ...seed,
        label: t(LABEL_MAP[seed.id]),
      })),
    [t],
  );

  const { visibleTabs, activeTab, setActiveTab } = useOpsTabUrl<InfraTabId>(
    tabDefs,
    MERGED_DEFAULT_TAB.infra,
  );

  const tabOptions = useMemo<TabOption<InfraTabId>[]>(
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

      {activeTab === "middleware" && (
        <div className="space-y-4 min-w-0">
          <HealthPanel
            mode="infra"
            timeRange={timeRange}
            reloadTick={reloadTick}
          />
          <InfraMetricsPanel
            tab="middleware"
            noteKey="ops.infra.middlewareNote"
            timeRange={timeRange}
            reloadTick={reloadTick}
          />
        </div>
      )}

      {activeTab === "hosts" && (
        <InfraMetricsPanel
          tab="hosts"
          noteKey="ops.infra.hostsNote"
          timeRange={timeRange}
          reloadTick={reloadTick}
        />
      )}
    </div>
  );
}
