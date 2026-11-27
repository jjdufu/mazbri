import React, { useMemo } from "react";
import { Tabs, type TabOption } from "@/components/ui";
import { useT } from "@/locales";
import {
  MERGED_DEFAULT_TAB,
  getMergedRouteTabs,
  type MergedTabId,
} from "@/utils/ops/merged-route-tabs";
import { useOpsTabUrl } from "../shared/opsHelpers";
import { AlertEventsPanel } from "./AlertEventsPanel";
import { AlertRulesPanel } from "./AlertRulesPanel";
import { AlertSilencesPanel } from "./AlertSilencesPanel";

type AlertsTabId = MergedTabId<"alerts">;

const LABEL_MAP: Record<AlertsTabId, string> = {
  events: "menus.mazbri.alertEvents",
  rules: "menus.mazbri.alertRules",
  silences: "menus.mazbri.alertSilences",
};

export function AlertsPage() {
  const { t } = useT();

  const tabDefs = useMemo(
    () =>
      getMergedRouteTabs("alerts").map((seed) => ({
        ...seed,
        label: t(LABEL_MAP[seed.id]),
      })),
    [t],
  );

  const { visibleTabs, activeTab, setActiveTab } = useOpsTabUrl<AlertsTabId>(
    tabDefs,
    MERGED_DEFAULT_TAB.alerts,
  );

  const tabOptions = useMemo<TabOption<AlertsTabId>[]>(
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
        <Tabs items={tabOptions} value={activeTab} onChange={setActiveTab} />
      )}

      {activeTab === "events" && <AlertEventsPanel />}
      {activeTab === "rules" && <AlertRulesPanel />}
      {activeTab === "silences" && <AlertSilencesPanel />}
    </div>
  );
}
