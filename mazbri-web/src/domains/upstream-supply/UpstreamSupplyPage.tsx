import { useEffect, useState } from "react";
import { useSearchParams } from "react-router";
import { Tabs, type TabOption } from "@/components/ui";
import { useT } from "@/locales";
import { ChannelListTab } from "./ChannelListTab";
import { ConnectorDraftsTab } from "./ConnectorDraftsTab";
import { SupplierListTab } from "./SupplierListTab";
import { UpstreamAccessWizardDrawer } from "./UpstreamAccessWizardDrawer";

type UpstreamSupplyTab = "suppliers" | "channels" | "drafts";

function resolveTab(raw: string | null): UpstreamSupplyTab {
  if (raw === "channels") return "channels";
  if (raw === "drafts") return "drafts";
  return "suppliers";
}

export function UpstreamSupplyPage() {
  const { t } = useT();
  const [searchParams, setSearchParams] = useSearchParams();
  const activeTab = resolveTab(searchParams.get("tab"));
  const [loadedTabs, setLoadedTabs] = useState<Set<UpstreamSupplyTab>>(
    () => new Set([activeTab]),
  );
  const [accessOpen, setAccessOpen] = useState(false);
  const [refreshToken, setRefreshToken] = useState(0);

  useEffect(() => {
    setLoadedTabs((prev) => {
      if (prev.has(activeTab)) return prev;
      const next = new Set(prev);
      next.add(activeTab);
      return next;
    });
  }, [activeTab]);

  const handleTabChange = (next: UpstreamSupplyTab) => {
    const nextParams = new URLSearchParams(searchParams);
    nextParams.set("tab", next);
    setSearchParams(nextParams, { replace: true });
  };

  const tabItems: TabOption<UpstreamSupplyTab>[] = [
    {
      key: "suppliers",
      label: t("menus.mazbri.supplierList"),
      testId: "tab-suppliers",
    },
    {
      key: "channels",
      label: t("menus.mazbri.upstreamChannelList"),
      testId: "tab-channels",
    },
    {
      key: "drafts",
      label: t("upstreamConnector.draftsTitle"),
      testId: "tab-drafts",
    },
  ];

  return (
    <div className="space-y-4 min-w-0" data-testid="upstream-supply-page">
      <Tabs items={tabItems} value={activeTab} onChange={handleTabChange} />

      {loadedTabs.has("suppliers") && (
        <div className={activeTab === "suppliers" ? "min-w-0" : "hidden"}>
          <SupplierListTab
            refreshToken={refreshToken}
            onOpenAccess={() => setAccessOpen(true)}
          />
        </div>
      )}

      {loadedTabs.has("channels") && (
        <div className={activeTab === "channels" ? "min-w-0" : "hidden"}>
          <ChannelListTab
            active={activeTab === "channels"}
            refreshToken={refreshToken}
          />
        </div>
      )}

      {loadedTabs.has("drafts") && (
        <div className={activeTab === "drafts" ? "min-w-0" : "hidden"}>
          <ConnectorDraftsTab />
        </div>
      )}

      <UpstreamAccessWizardDrawer
        open={accessOpen}
        onClose={() => setAccessOpen(false)}
        onSuccess={() => setRefreshToken((n) => n + 1)}
      />
    </div>
  );
}
