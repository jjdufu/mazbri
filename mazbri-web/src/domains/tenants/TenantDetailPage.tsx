import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router";
import {
  Activity,
  Building2,
  Package,
  Power,
  RefreshCw,
  ShieldCheck,
  Wallet,
} from "lucide-react";
import { fetchTenantById, setTenantEnabled } from "@/api/tenant";
import {
  Button,
  ConfirmDialog,
  EntityDetailLayout,
  type EntityDetailMetaItem,
  type TabOption,
} from "@/components/ui";
import { LanguageEnum } from "@/enums/appEnum";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import type { Tenant } from "@/types/api/tenant";
import { getStatusLabel } from "@/utils/display-labels";
import { EMPTY_PLACEHOLDER, formatDateTime } from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { isBizRole } from "@/utils/ops/roles";
import { logger } from "@/utils/sys/logger";
import { normalizeTenantCode } from "@/utils/tenant-code";
import { showSuccessToast } from "@/utils/ui/message-guard";
import { AccountTab } from "./tabs/AccountTab";
import { BasicAndKeysTab } from "./tabs/BasicAndKeysTab";
import { Overview360Tab } from "./tabs/Overview360Tab";
import { ProductsTab } from "./tabs/ProductsTab";
import { SecurityPoliciesTab } from "./tabs/SecurityPoliciesTab";

type TenantDetailTabKey =
  | "overview"
  | "basic"
  | "security"
  | "products"
  | "account";

function normalizeTabParam(
  raw: string | null,
  canViewAccount: boolean,
): TenantDetailTabKey {
  switch (raw) {
    case "overview":
    case "overview360":
      return "overview";
    case "basic":
    case "api-keys":
      return "basic";
    case "security":
    case "whitelist":
    case "access-strategy":
    case "transport-encryption":
      return "security";
    case "products":
      return "products";
    case "account":
      return canViewAccount ? "account" : "overview";
    default:
      return "overview";
  }
}

export function TenantDetailPage() {
  const { t, locale } = useT();
  const isZh = locale === LanguageEnum.ZH;
  const navigate = useNavigate();
  const params = useParams<{ code: string }>();
  const [searchParams, setSearchParams] = useSearchParams();
  const role = useAuthStore((s) => s.info.role);
  const canWrite = isBizRole(role);

  const tenantCode = normalizeTenantCode(params.code) ?? "";
  const rawTab = searchParams.get("tab");
  const activeTab = useMemo(
    () => normalizeTabParam(rawTab, canWrite),
    [rawTab, canWrite],
  );

  // Normalize legacy tab aliases in URL without adding history entries
  useEffect(() => {
    if (rawTab && rawTab !== activeTab) {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          next.set("tab", activeTab);
          return next;
        },
        { replace: true },
      );
    }
  }, [rawTab, activeTab, setSearchParams]);

  const [visitedTabs, setVisitedTabs] = useState<Set<TenantDetailTabKey>>(
    () => new Set([activeTab]),
  );

  useEffect(() => {
    setVisitedTabs((prev) => {
      if (prev.has(activeTab)) return prev;
      const next = new Set(prev);
      next.add(activeTab);
      return next;
    });
  }, [activeTab]);

  // Reset visited tabs when navigating to a different tenant
  const prevTenantCodeRef = useRef(tenantCode);
  useEffect(() => {
    if (prevTenantCodeRef.current !== tenantCode) {
      prevTenantCodeRef.current = tenantCode;
      setVisitedTabs(new Set([activeTab]));
    }
  }, [tenantCode, activeTab]);

  const [tenant, setTenant] = useState<Tenant | null>(null);
  const [loading, setLoading] = useState(false);
  const [toggleConfirmOpen, setToggleConfirmOpen] = useState(false);
  const [toggling, setToggling] = useState(false);

  const abortRef = useRef<AbortController | null>(null);

  const loadTenant = useCallback(async () => {
    if (!tenantCode) {
      setTenant(null);
      return;
    }
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;
    setLoading(true);
    try {
      const res = await fetchTenantById(tenantCode, controller.signal);
      if (controller.signal.aborted) return;
      setTenant(res);
    } catch (error) {
      if (!controller.signal.aborted && !isMfaStepUpCancelled(error)) {
        logger.error("Failed to load tenant:", error);
      }
    } finally {
      if (!controller.signal.aborted) {
        setLoading(false);
      }
    }
  }, [tenantCode]);

  useEffect(() => {
    void loadTenant();
    return () => abortRef.current?.abort();
  }, [loadTenant]);

  const handleTabChange = useCallback(
    (nextTab: TenantDetailTabKey) => {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          next.set("tab", nextTab);
          return next;
        },
        { replace: true },
      );
    },
    [setSearchParams],
  );

  const handleConfirmToggle = async () => {
    if (!tenant || toggling) return;
    const nextEnabled = !tenant.enabled;
    setToggling(true);
    try {
      await setTenantEnabled(tenant.tenant_code, { enabled: nextEnabled });
      showSuccessToast(
        nextEnabled
          ? t("common.feedback.enabled")
          : t("common.feedback.disabled"),
      );
      setToggleConfirmOpen(false);
      void loadTenant();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to toggle tenant status:", error);
      }
    } finally {
      setToggling(false);
    }
  };

  const tabs = useMemo<TabOption<TenantDetailTabKey>[]>(() => {
    const list: TabOption<TenantDetailTabKey>[] = [
      {
        key: "overview",
        label: t("tenant.detail.tabs.overview360"),
        icon: <Activity className="h-3.5 w-3.5" />,
        testId: "tenant-tab-overview",
      },
      {
        key: "basic",
        label: isZh ? "基础与凭据" : "Basic & Credentials",
        icon: <Building2 className="h-3.5 w-3.5" />,
        testId: "tenant-tab-basic",
      },
      {
        key: "security",
        label: isZh ? "接入与安全策略" : "Access & Security",
        icon: <ShieldCheck className="h-3.5 w-3.5" />,
        testId: "tenant-tab-security",
      },
      {
        key: "products",
        label: t("tenant.detail.tabs.products"),
        icon: <Package className="h-3.5 w-3.5" />,
        testId: "tenant-tab-products",
      },
    ];
    if (canWrite) {
      list.push({
        key: "account",
        label: isZh ? "账户与流水" : "Account & Ledger",
        icon: <Wallet className="h-3.5 w-3.5" />,
        testId: "tenant-tab-account",
      });
    }
    return list;
  }, [canWrite, isZh, t]);

  const meta = useMemo<EntityDetailMetaItem[]>(
    () => [
      {
        label: t("tenant.fields.accountId"),
        value: (
          <span className="font-mono-tech">
            {tenant?.tenant_id || EMPTY_PLACEHOLDER}
          </span>
        ),
      },
      {
        label: t("tenant.fields.contactPerson"),
        value: tenant?.contact_person || EMPTY_PLACEHOLDER,
      },
      {
        label: t("common.fields.createdAt"),
        value: (
          <span className="font-mono-tech">
            {formatDateTime(tenant?.created_at)}
          </span>
        ),
      },
    ],
    [tenant, t],
  );

  const statusKey = tenant?.enabled === false ? "disabled" : "enabled";

  return (
    <>
      <EntityDetailLayout<TenantDetailTabKey>
        testId="tenant-detail-page"
        title={tenant?.name || tenantCode}
        code={tenantCode}
        status={statusKey === "disabled" ? "DISABLED" : "ENABLED"}
        statusLabel={getStatusLabel(statusKey)}
        meta={meta}
        onBack={() => navigate("/operations/tenants")}
        backLabel={t("common.actions.back")}
        tabs={tabs}
        activeTab={activeTab}
        onTabChange={handleTabChange}
        actions={
          <>
            <Button
              type="button"
              variant="outline"
              size="md"
              onClick={() => void loadTenant()}
              disabled={loading}
            >
              <RefreshCw
                className={loading ? "h-3.5 w-3.5 animate-spin" : "h-3.5 w-3.5"}
              />
              {t("common.actions.refresh")}
            </Button>
            {canWrite && tenant && (
              <Button
                type="button"
                variant={tenant.enabled ? "warning" : "primary"}
                size="md"
                onClick={() => setToggleConfirmOpen(true)}
              >
                <Power className="h-3.5 w-3.5" />
                {tenant.enabled
                  ? t("tenant.actions.disable")
                  : t("tenant.actions.enable")}
              </Button>
            )}
          </>
        }
      >
        {tenantCode && visitedTabs.has("overview") && (
          <div className={activeTab === "overview" ? "block" : "hidden"}>
            <Overview360Tab tenantCode={tenantCode} />
          </div>
        )}

        {visitedTabs.has("basic") && (
          <div className={activeTab === "basic" ? "block" : "hidden"}>
            <BasicAndKeysTab
              tenantCode={tenantCode}
              tenant={tenant}
              loading={loading}
              onRefreshTenant={() => void loadTenant()}
            />
          </div>
        )}

        {tenantCode && visitedTabs.has("security") && (
          <div className={activeTab === "security" ? "block" : "hidden"}>
            <SecurityPoliciesTab tenantCode={tenantCode} />
          </div>
        )}

        {tenantCode && visitedTabs.has("products") && (
          <div className={activeTab === "products" ? "block" : "hidden"}>
            <ProductsTab tenantCode={tenantCode} />
          </div>
        )}

        {canWrite && tenantCode && visitedTabs.has("account") && (
          <div className={activeTab === "account" ? "block" : "hidden"}>
            <AccountTab tenantCode={tenantCode} />
          </div>
        )}
      </EntityDetailLayout>

      <ConfirmDialog
        open={toggleConfirmOpen}
        onClose={() => setToggleConfirmOpen(false)}
        onConfirm={() => void handleConfirmToggle()}
        loading={toggling}
        danger={Boolean(tenant?.enabled)}
        title={t("common.tips")}
        confirmLabel={t("common.confirm")}
        description={
          tenant
            ? tenant.enabled
              ? t("tenant.messages.disableTenantConfirm", { name: tenant.name })
              : t("tenant.messages.enableTenantConfirm", { name: tenant.name })
            : ""
        }
      />
    </>
  );
}
