import { useCallback, useEffect, useState } from "react";
import { useSearchParams } from "react-router";
import { Award, Settings, ShieldCheck } from "lucide-react";
import { PageHeaderActions } from "@/components/layout";
import { ConfirmDialog, Tabs } from "@/components/ui";
import { useT } from "@/locales";
import { BasicSettingsTab } from "./BasicSettingsTab";
import { PlatformLicenseTab } from "./PlatformLicenseTab";
import { SecurityPolicyTab } from "./SecurityPolicyTab";

type PlatformTab = "basic" | "security" | "license";

const VALID_TABS: readonly PlatformTab[] = ["basic", "security", "license"];

function parseTab(raw: string | null): PlatformTab {
  if (raw && (VALID_TABS as readonly string[]).includes(raw)) {
    return raw as PlatformTab;
  }
  return "basic";
}

export function SystemSettingsPage() {
  const { t } = useT();
  const [searchParams, setSearchParams] = useSearchParams();
  const activeTab = parseTab(searchParams.get("tab"));

  const [tabDirty, setTabDirty] = useState(false);
  const [pendingTab, setPendingTab] = useState<PlatformTab | null>(null);

  useEffect(() => {
    const onBeforeUnload = (e: BeforeUnloadEvent) => {
      if (!tabDirty) return;
      e.preventDefault();
      e.returnValue = "";
    };
    window.addEventListener("beforeunload", onBeforeUnload);
    return () => window.removeEventListener("beforeunload", onBeforeUnload);
  }, [tabDirty]);

  const commitTabChange = useCallback(
    (next: PlatformTab) => {
      setTabDirty(false);
      setPendingTab(null);
      setSearchParams(
        (prev) => {
          const updated = new URLSearchParams(prev);
          updated.set("tab", next);
          return updated;
        },
        { replace: true },
      );
    },
    [setSearchParams],
  );

  const handleTabChange = (next: PlatformTab) => {
    if (next === activeTab) return;
    if (tabDirty) {
      setPendingTab(next);
      return;
    }
    commitTabChange(next);
  };

  return (
    <div className="space-y-4 min-w-0" data-testid="system-settings-page">
      <PageHeaderActions>
        <Tabs<PlatformTab>
          variant="inline"
          value={activeTab}
          onChange={handleTabChange}
          items={[
            {
              key: "basic",
              label: t("systemSettings.tabs.basic"),
              icon: <Settings className="h-3.5 w-3.5" />,
              testId: "system-tab-basic",
            },
            {
              key: "security",
              label: t("systemSettings.tabs.security"),
              icon: <ShieldCheck className="h-3.5 w-3.5" />,
              testId: "system-tab-security",
            },
            {
              key: "license",
              label: t("systemSettings.tabs.license"),
              icon: <Award className="h-3.5 w-3.5" />,
              testId: "system-tab-license",
            },
          ]}
        />
      </PageHeaderActions>

      {activeTab === "basic" && (
        <BasicSettingsTab onDirtyChange={setTabDirty} />
      )}
      {activeTab === "security" && (
        <SecurityPolicyTab onDirtyChange={setTabDirty} />
      )}
      {activeTab === "license" && <PlatformLicenseTab />}

      <ConfirmDialog
        open={pendingTab !== null}
        onClose={() => setPendingTab(null)}
        onConfirm={() => {
          if (pendingTab) commitTabChange(pendingTab);
        }}
        title={t("systemSettings.unsavedChanges")}
        description={t("systemSettings.unsavedLeaveConfirm")}
        confirmLabel={t("common.confirm")}
        cancelLabel={t("common.cancel")}
        danger={false}
      />
    </div>
  );
}
