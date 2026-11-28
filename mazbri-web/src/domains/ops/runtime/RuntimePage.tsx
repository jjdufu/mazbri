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
import {
  useOpsRole,
  useOpsTabUrl,
  useOpsTimeRangeUrl,
} from "../shared/opsHelpers";
import { BillingPipelinePanel } from "./BillingPipelinePanel";
import { ControlPlanePipelinePanel } from "./ControlPlanePipelinePanel";
import { HealthPanel } from "./HealthPanel";
import { InfraMetricsPanel } from "./InfraMetricsPanel";

type RuntimeTabId = MergedTabId<"runtime">;

const LABEL_MAP: Record<RuntimeTabId, string> = {
  apps: "menus.mazbri.runtimeApps",
  middleware: "menus.mazbri.infraMiddleware",
  pipeline: "menus.mazbri.infraPipeline",
  hosts: "menus.mazbri.infraHosts",
};

export function RuntimePage() {
  const { t } = useT();
  const { isAdmin, isBiz } = useOpsRole();
  const { timeRange, setTimeRange } = useOpsTimeRangeUrl("1h");
  const [refreshInterval, setRefreshInterval] = useState(30);
  const [reloadTick, setReloadTick] = useState(0);

  const tabDefs = useMemo(
    () =>
      getMergedRouteTabs("runtime").map((seed) => ({
        ...seed,
        label: t(LABEL_MAP[seed.id]),
      })),
    [t],
  );

  const { visibleTabs, activeTab, setActiveTab } = useOpsTabUrl<RuntimeTabId>(
    tabDefs,
    MERGED_DEFAULT_TAB.runtime,
  );

  const tabOptions = useMemo<TabOption<RuntimeTabId>[]>(
    () =>
      visibleTabs.map((item) => ({
        key: item.id,
        label: item.label ?? item.id,
      })),
    [visibleTabs],
  );

  return (
    <div className="flex flex-col flex-1 min-h-0 gap-4 min-w-0">
      {tabOptions.length > 1 && (
        <PageHeaderActions>
          <Tabs
            variant="inline"
            items={tabOptions}
            value={activeTab}
            onChange={setActiveTab}
          />
        </PageHeaderActions>
      )}
      <div className="flex flex-wrap items-center justify-end gap-2 shrink-0">
        <OpsTimeBar
          value={timeRange}
          onChange={setTimeRange}
          refreshInterval={refreshInterval}
          onRefreshIntervalChange={setRefreshInterval}
          onRefresh={() => setReloadTick((v) => v + 1)}
        />
      </div>

      {activeTab === "apps" && (
        <HealthPanel
          mode="apps"
          timeRange={timeRange}
          reloadTick={reloadTick}
        />
      )}

      {activeTab === "middleware" && (
        <div className="flex flex-col flex-1 min-h-0 gap-4 min-w-0">
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

      {activeTab === "pipeline" && (
        <div className="flex flex-col flex-1 min-h-0 gap-4 min-w-0">
          {isBiz && (
            <>
              <BillingPipelinePanel
                timeRange={timeRange}
                reloadTick={reloadTick}
              />
              <ControlPlanePipelinePanel
                timeRange={timeRange}
                reloadTick={reloadTick}
              />
            </>
          )}
          {isAdmin && (
            <InfraMetricsPanel
              tab="pipeline"
              noteKey="ops.infra.pipelineNote"
              timeRange={timeRange}
              reloadTick={reloadTick}
            />
          )}
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
