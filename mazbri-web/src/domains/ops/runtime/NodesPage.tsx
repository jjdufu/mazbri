import React, { useState } from "react";
import { PageHeaderActions } from "@/components/layout";
import { OpsTimeBar } from "../shared/OpsTimeBar";
import { useOpsTimeRangeUrl } from "../shared/opsHelpers";
import { HealthPanel } from "./HealthPanel";

export function NodesPage() {
  const { timeRange, setTimeRange } = useOpsTimeRangeUrl("1h");
  const [refreshInterval, setRefreshInterval] = useState(30);
  const [reloadTick, setReloadTick] = useState(0);

  return (
    <div className="flex flex-col flex-1 min-h-0 gap-4 min-w-0">
      <PageHeaderActions>
        <OpsTimeBar
          value={timeRange}
          onChange={setTimeRange}
          refreshInterval={refreshInterval}
          onRefreshIntervalChange={setRefreshInterval}
          onRefresh={() => setReloadTick((v) => v + 1)}
        />
      </PageHeaderActions>

      <HealthPanel
        mode="apps"
        timeRange={timeRange}
        reloadTick={reloadTick}
      />
    </div>
  );
}
