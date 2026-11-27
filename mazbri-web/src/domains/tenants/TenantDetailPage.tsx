import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router";
import {
  Activity,
  Gauge,
  Package,
  Pencil,
  Power,
  RefreshCw,
  ShieldCheck,
  Wallet,
} from "lucide-react";
import { fetchTenantById, setTenantEnabled, updateTenant } from "@/api/tenant";
import {
  Button,
  ConfirmDialog,
  CopyableText,
  EntityDetailLayout,
  FormField,
  FormModal,
  Input,
  type EntityDetailMetaItem,
  type TabOption,
} from "@/components/ui";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import type { Tenant, UpdateTenantRequest } from "@/types/api/tenant";
import { getStatusLabel } from "@/utils/display-labels";
import { EMPTY_PLACEHOLDER, formatDateTime } from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { isBizRole } from "@/utils/ops/roles";
import { logger } from "@/utils/sys/logger";
import { normalizeTenantCode } from "@/utils/tenant-code";
import { showSuccessToast } from "@/utils/ui/message-guard";
import { AccountTab } from "./tabs/AccountTab";
import { CredentialsAndSecurityTab } from "./tabs/CredentialsAndSecurityTab";
import { Overview360Tab } from "./tabs/Overview360Tab";
import { ProductsTab } from "./tabs/ProductsTab";
import { RateLimitAndCacheTab } from "./tabs/RateLimitAndCacheTab";

type TenantDetailTabKey =
  | "overview"
  | "credentials"
  | "policies"
  | "products"
  | "account";

const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

function normalizeTabParam(
  raw: string | null,
  canViewAccount: boolean,
): TenantDetailTabKey {
  switch (raw) {
    case "overview":
    case "overview360":
      return "overview";
    case "credentials":
    case "basic":
    case "api-keys":
    case "security":
    case "whitelist":
    case "transport-encryption":
      return "credentials";
    case "policies":
    case "access-strategy":
    case "ratelimit":
    case "cache":
      return "policies";
    case "products":
      return "products";
    case "account":
      return canViewAccount ? "account" : "overview";
    default:
      return "overview";
  }
}

export function TenantDetailPage() {
  const { t } = useT();
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

  const [editOpen, setEditOpen] = useState(false);
  const [savingBasic, setSavingBasic] = useState(false);
  const [editForm, setEditForm] = useState<UpdateTenantRequest>({
    name: "",
    contact_person: "",
    contact_phone: "",
    contact_email: "",
  });
  const [editErrors, setEditErrors] = useState<{
    name?: string;
    contact_email?: string;
  }>({});

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

  const handleOpenEdit = () => {
    if (!tenant) return;
    setEditForm({
      name: tenant.name,
      contact_person: tenant.contact_person ?? "",
      contact_phone: tenant.contact_phone ?? "",
      contact_email: tenant.contact_email ?? "",
    });
    setEditErrors({});
    setEditOpen(true);
  };

  const handleSaveBasic = async () => {
    if (!tenant || savingBasic) return;
    const nextErrors: { name?: string; contact_email?: string } = {};
    const name = (editForm.name ?? "").trim();
    const email = (editForm.contact_email ?? "").trim();
    if (!name) {
      nextErrors.name = t("tenant.detail.validation.nameRequired");
    } else if (name.length < 2 || name.length > 100) {
      nextErrors.name = t("tenant.detail.validation.nameLength");
    }
    if (email && !EMAIL_RE.test(email)) {
      nextErrors.contact_email = t("tenant.detail.validation.email");
    }
    setEditErrors(nextErrors);
    if (Object.keys(nextErrors).length > 0) return;

    setSavingBasic(true);
    try {
      await updateTenant(tenant.tenant_code, {
        name,
        contact_person: (editForm.contact_person ?? "").trim(),
        contact_phone: (editForm.contact_phone ?? "").trim(),
        contact_email: email,
      });
      showSuccessToast(t("common.feedback.saveSuccess"));
      setEditOpen(false);
      void loadTenant();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to update tenant:", error);
      }
    } finally {
      setSavingBasic(false);
    }
  };

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
        key: "credentials",
        label: t("tenant.detail.tabs.credentialsAndSecurity"),
        icon: <ShieldCheck className="h-3.5 w-3.5" />,
        testId: "tenant-tab-credentials",
      },
      {
        key: "policies",
        label: t("tenant.detail.tabs.rateLimitAndCache"),
        icon: <Gauge className="h-3.5 w-3.5" />,
        testId: "tenant-tab-policies",
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
        label: t("tenant.detail.tabs.account"),
        icon: <Wallet className="h-3.5 w-3.5" />,
        testId: "tenant-tab-account",
      });
    }
    return list;
  }, [canWrite, t]);

  const meta = useMemo<EntityDetailMetaItem[]>(
    () => [
      {
        label: t("tenant.fields.accountId"),
        value: tenant?.tenant_id ? (
          <CopyableText value={tenant.tenant_id} mono />
        ) : (
          EMPTY_PLACEHOLDER
        ),
      },
      {
        label: t("tenant.fields.contactPerson"),
        value: tenant?.contact_person || EMPTY_PLACEHOLDER,
      },
      {
        label: t("tenant.fields.contactPhone"),
        value: (
          <span className="font-mono-tech">
            {tenant?.contact_phone || EMPTY_PLACEHOLDER}
          </span>
        ),
      },
      {
        label: t("tenant.fields.contactEmail"),
        value: tenant?.contact_email || EMPTY_PLACEHOLDER,
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
            {canWrite && (
              <Button
                type="button"
                variant="outline"
                size="md"
                disabled={!tenant || loading}
                onClick={handleOpenEdit}
                data-testid="tenant-basic-edit-btn"
              >
                <Pencil className="h-3.5 w-3.5" />
                {t("common.actions.edit")}
              </Button>
            )}
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

        {tenantCode && visitedTabs.has("credentials") && (
          <div className={activeTab === "credentials" ? "block" : "hidden"}>
            <CredentialsAndSecurityTab tenantCode={tenantCode} />
          </div>
        )}

        {tenantCode && visitedTabs.has("policies") && (
          <div className={activeTab === "policies" ? "block" : "hidden"}>
            <RateLimitAndCacheTab tenantCode={tenantCode} />
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

      <FormModal
        open={editOpen}
        onClose={() => setEditOpen(false)}
        onSubmit={handleSaveBasic}
        title={t("tenant.detail.basicInfo.editTitle")}
        size="md"
        submitting={savingBasic}
        testId="tenant-basic-edit-modal"
      >
        <FormField
          label={t("common.fields.name")}
          htmlFor="edit-tenant-name"
          required
          error={editErrors.name}
        >
          <Input
            id="edit-tenant-name"
            value={editForm.name ?? ""}
            onChange={(e) => {
              setEditForm((prev) => ({ ...prev, name: e.target.value }));
              if (editErrors.name) {
                setEditErrors((prev) => ({ ...prev, name: undefined }));
              }
            }}
          />
        </FormField>

        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <FormField
            label={t("tenant.fields.contactPerson")}
            htmlFor="edit-tenant-contact-person"
          >
            <Input
              id="edit-tenant-contact-person"
              value={editForm.contact_person ?? ""}
              onChange={(e) =>
                setEditForm((prev) => ({
                  ...prev,
                  contact_person: e.target.value,
                }))
              }
            />
          </FormField>

          <FormField
            label={t("tenant.fields.contactPhone")}
            htmlFor="edit-tenant-contact-phone"
          >
            <Input
              id="edit-tenant-contact-phone"
              value={editForm.contact_phone ?? ""}
              onChange={(e) =>
                setEditForm((prev) => ({
                  ...prev,
                  contact_phone: e.target.value,
                }))
              }
              className="font-mono-tech"
            />
          </FormField>
        </div>

        <FormField
          label={t("tenant.fields.contactEmail")}
          htmlFor="edit-tenant-contact-email"
          error={editErrors.contact_email}
        >
          <Input
            id="edit-tenant-contact-email"
            type="email"
            value={editForm.contact_email ?? ""}
            onChange={(e) => {
              setEditForm((prev) => ({
                ...prev,
                contact_email: e.target.value,
              }));
              if (editErrors.contact_email) {
                setEditErrors((prev) => ({
                  ...prev,
                  contact_email: undefined,
                }));
              }
            }}
          />
        </FormField>
      </FormModal>

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
