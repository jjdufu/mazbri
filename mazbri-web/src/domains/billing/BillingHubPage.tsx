import { useCallback, useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router";
import { BarChart3, CheckSquare, FileText } from "lucide-react";
import { PageHeaderActions } from "@/components/layout";
import { Select, Tabs, type SelectOptionItem, type TabOption } from "@/components/ui";
import { AutoRefreshControl } from "@/domains/ops/shared/OpsTimeBar";
import { useT } from "@/locales";
import { BillListPanel } from "./panels/BillListPanel";
import { FinanceOverviewPanel } from "./panels/FinanceOverviewPanel";
import { UsageReconciliationPanel } from "./panels/UsageReconciliationPanel";

type BillingHubTab = "overview" | "bills" | "reconcile";

function resolveTab(raw: string | null): BillingHubTab {
  if (raw === "overview" || raw === "bills" || raw === "reconcile") {
    return raw;
  }
  return "overview";
}

export function BillingHubPage() {
  const { t } = useT();
  const [searchParams, setSearchParams] = useSearchParams();
  const rawTab = searchParams.get("tab");
  const activeTab = resolveTab(rawTab);

  const [overviewDays, setOverviewDays] = useState<number>(30);
  const [overviewRefresh, setOverviewRefresh] = useState<number>(60);
  const [overviewReloadTick, setOverviewReloadTick] = useState<number>(0);
  const [overviewLoading, setOverviewLoading] = useState<boolean>(false);

  const [visited, setVisited] = useState<Set<BillingHubTab>>(
    () => new Set([activeTab]),
  );

  useEffect(() => {
    setVisited((prev) => {
      if (prev.has(activeTab)) return prev;
      const next = new Set(prev);
      next.add(activeTab);
      return next;
    });
  }, [activeTab]);

  const handleTabChange = useCallback(
    (next: BillingHubTab) => {
      setSearchParams(
        (prev) => {
          const params = new URLSearchParams(prev);
          params.set("tab", next);
          return params;
        },
        { replace: true },
      );
    },
    [setSearchParams],
  );

  const daysOptions = useMemo<SelectOptionItem[]>(
    () => [
      {
        value: "7",
        label: `7 ${t("ops.units.days")}`,
        text: `7 ${t("ops.units.days")}`,
      },
      {
        value: "30",
        label: `30 ${t("ops.units.days")}`,
        text: `30 ${t("ops.units.days")}`,
      },
      {
        value: "90",
        label: `90 ${t("ops.units.days")}`,
        text: `90 ${t("ops.units.days")}`,
      },
    ],
    [t],
  );

  const tabs = useMemo<TabOption<BillingHubTab>[]>(
    () => [
      {
        key: "overview",
        label: t("ops.finance.tabOverview"),
        icon: <BarChart3 className="h-3.5 w-3.5" />,
        testId: "billing-tab-overview",
      },
      {
        key: "bills",
        label: t("ops.finance.tabBills"),
        icon: <FileText className="h-3.5 w-3.5" />,
        testId: "billing-tab-bills",
      },
      {
        key: "reconcile",
        label: t("ops.finance.tabReconcile"),
        icon: <CheckSquare className="h-3.5 w-3.5" />,
        testId: "billing-tab-reconcile",
      },
    ],
    [t],
  );

  return (
    <div
      className="flex flex-col flex-1 min-h-0 gap-4 min-w-0"
      data-testid="billing-hub-page"
    >
      <PageHeaderActions>
        <div className="flex flex-wrap items-center justify-end gap-2.5">
          {activeTab === "overview" && (
            <>
              <div className="flex items-center gap-1.5">
                <Select
                  value={String(overviewDays)}
                  options={daysOptions}
                  onChange={(e) => setOverviewDays(Number(e.target.value) || 30)}
                  className="w-24"
                  aria-label={t("ops.units.days")}
                />
                <AutoRefreshControl
                  refreshInterval={overviewRefresh}
                  onRefreshIntervalChange={setOverviewRefresh}
                  onRefresh={() => setOverviewReloadTick((n) => n + 1)}
                  loading={overviewLoading}
                  includeFiveMinutes
                />
              </div>
              <span
                className="hidden sm:inline-block h-4 w-px bg-border-subtle"
                aria-hidden="true"
              />
            </>
          )}
          <Tabs
            variant="inline"
            items={tabs}
            value={activeTab}
            onChange={handleTabChange}
          />
        </div>
      </PageHeaderActions>

      {visited.has("overview") && (
        <div
          className={
            activeTab === "overview" ? "flex flex-col flex-1 min-h-0" : "hidden"
          }
        >
          <FinanceOverviewPanel
            active={activeTab === "overview"}
            days={overviewDays}
            refreshSeconds={overviewRefresh}
            reloadTick={overviewReloadTick}
            onLoadingChange={setOverviewLoading}
          />
        </div>
      )}

      {visited.has("bills") && (
        <div
          className={
            activeTab === "bills" ? "flex flex-col flex-1 min-h-0" : "hidden"
          }
        >
          <BillListPanel active={activeTab === "bills"} />
        </div>
      )}

      {visited.has("reconcile") && (
        <div
          className={
            activeTab === "reconcile"
              ? "flex flex-col flex-1 min-h-0"
              : "hidden"
          }
        >
          <UsageReconciliationPanel active={activeTab === "reconcile"} />
        </div>
      )}
    </div>
  );
}
