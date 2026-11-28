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
import { InfraMetricsPanel } from "./InfraMetricsPanel";

type PipelineTabId = MergedTabId<"pipeline">;

const LABEL_MAP: Record<PipelineTabId, string> = {
  billing: "menus.mazbri.pipelineBilling",
  control: "menus.mazbri.pipelineControl",
  storage: "menus.mazbri.pipelineStorage",
};

export function PipelinePage() {
  const { t } = useT();
  const { isAdmin, isBiz } = useOpsRole();
  const { timeRange, setTimeRange } = useOpsTimeRangeUrl("1h");
  const [refreshInterval, setRefreshInterval] = useState(30);
  const [reloadTick, setReloadTick] = useState(0);

  const tabDefs = useMemo(
    () =>
      getMergedRouteTabs("pipeline").map((seed) => ({
        ...seed,
        label: t(LABEL_MAP[seed.id]),
      })),
    [t],
  );

  const { visibleTabs, activeTab, setActiveTab } = useOpsTabUrl<PipelineTabId>(
    tabDefs,
    MERGED_DEFAULT_TAB.pipeline,
  );

  const tabOptions = useMemo<TabOption<PipelineTabId>[]>(
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

      {activeTab === "billing" && isBiz && (
        <BillingPipelinePanel
          timeRange={timeRange}
          reloadTick={reloadTick}
        />
      )}

      {activeTab === "control" && isBiz && (
        <ControlPlanePipelinePanel
          timeRange={timeRange}
          reloadTick={reloadTick}
        />
      )}

      {activeTab === "storage" && isAdmin && (
        <InfraMetricsPanel
          tab="pipeline"
          noteKey="ops.infra.pipelineNote"
          timeRange={timeRange}
          reloadTick={reloadTick}
        />
      )}
    </div>
  );
}
