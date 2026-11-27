import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Plus,
  RefreshCw,
  Search,
  Trash2,
  Upload,
  Zap,
} from "lucide-react";
import { fetchAuthorizations, fetchProductList } from "@/api/product";
import {
  createRouteBinding,
  createRouteRule,
  deleteRouteBinding,
  deleteRouteRule,
  emergencySwitchRoute,
  fetchChannels,
  fetchRouteBindings,
  fetchRouteRules,
  updateRouteBinding,
  updateRouteRule,
  type Channel,
  type RouteBinding,
  type RouteRule,
} from "@/api/route-management";
import { CsvImportModal } from "@/components/common/CsvImportModal";
import { ExportDropdown } from "@/components/common/ExportDropdown";
import {
  Badge,
  Button,
  ConfirmDialog,
  CopyableText,
  DataTablePage,
  FormField,
  FormModal,
  Input,
  Modal,
  NumberInput,
  SegmentedControl,
  Select,
  StatusBadge,
  Switch,
  Textarea,
  type DataTableColumn,
  type SelectOptionItem,
} from "@/components/ui";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import type { AuthorizationDetail, Product } from "@/types/api/product";
import type { ListFilter } from "@/types/list-query";
import { EMPTY_PLACEHOLDER, formatDateTime } from "@/utils/format";
import { getUserFacingErrorMessage } from "@/utils/http/error";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { isBizRole } from "@/utils/ops/roles";
import {
  exportResourceCsv,
  runResourceCsvImport,
  type ColumnConfig,
} from "@/utils/resource-csv";
import { logger } from "@/utils/sys/logger";
import {
  showErrorToast,
  showInfoToast,
  showSuccessToast,
  showWarningToast,
} from "@/utils/ui/message-guard";
import { EmergencySwitchModal } from "./EmergencySwitchModal";
import {
  DEFAULT_FAILURE_TYPES,
  DEFAULT_MAX_RETRIES,
  DEFAULT_TIMEOUT_MS,
  channelStatusLabel,
  dedupeTenants,
  failureTypeLabel,
  getFailureTypeOptions,
  getNumber,
  getRecord,
  getRoutePurposeOptions,
  getString,
  getStringArray,
  hasMetadata,
  isEmergencySwitchEnabled,
  isExampleRoute,
  routePurposeLabel,
  routePurposeLabelFromMetadata,
  routeStatusBadgeKey,
  routeStatusLabel,
  switchConditionSummary,
  type TenantRouteOption,
} from "./helpers";
import type {
  BindingFormState,
  RouteFormState,
  RouteImportRow,
} from "./types";

export interface ProductRoutesSectionProps {
  embedded?: boolean;
  initialProductId?: string;
  lockProduct?: boolean;
  borderless?: boolean;
}

const DEFAULT_ROUTE_FORM = (productId: string): RouteFormState => ({
  scope: "tenant",
  tenant_id: "",
  product_id: productId,
  priority: 100,
  status: "active",
  route_purpose: "standard",
  effective_mode: "immediate",
  effective_range: [],
  channel_id: "",
  timeout_ms: DEFAULT_TIMEOUT_MS,
  max_retries: DEFAULT_MAX_RETRIES,
});

const DEFAULT_BINDING_FORM = (order: number): BindingFormState => ({
  channel_id: "",
  channel_order: order,
  timeout_ms: DEFAULT_TIMEOUT_MS,
  max_retries: DEFAULT_MAX_RETRIES,
  failover_enabled: true,
  failure_types: [...DEFAULT_FAILURE_TYPES],
  description: "",
});

function toDatetimeLocalValue(iso: string): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso.slice(0, 16);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function toIsoFromDatetimeLocal(localVal: string): string {
  if (!localVal) return "";
  const d = new Date(localVal);
  if (Number.isNaN(d.getTime())) return localVal;
  return d.toISOString();
}

export function ProductRoutesSection({
  embedded = false,
  initialProductId = "",
  lockProduct: lockProductProp = false,
  borderless = false,
}: ProductRoutesSectionProps) {
  const { t } = useT();
  const role = useAuthStore((s) => s.info.role);
  const canWrite = isBizRole(role);
  const lockProduct = lockProductProp && Boolean(initialProductId);

  const [loading, setLoading] = useState(false);
  const [tableData, setTableData] = useState<RouteRule[]>([]);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [total, setTotal] = useState(0);
  const [keywordInput, setKeywordInput] = useState("");
  const [appliedKeyword, setAppliedKeyword] = useState("");
  const [statusFilter, setStatusFilter] = useState("");
  const [selectedIds, setSelectedIds] = useState<string[]>([]);

  const [channelRows, setChannelRows] = useState<Channel[]>([]);
  const [productRows, setProductRows] = useState<Product[]>([]);
  const [authorizations, setAuthorizations] = useState<AuthorizationDetail[]>(
    [],
  );

  // Route create/edit modal
  const [routeModalOpen, setRouteModalOpen] = useState(false);
  const [editingRoute, setEditingRoute] = useState<RouteRule | null>(null);
  const [routeForm, setRouteForm] = useState<RouteFormState>(() =>
    DEFAULT_ROUTE_FORM(initialProductId),
  );
  const [routeErrors, setRouteErrors] = useState<Record<string, string>>({});
  const [routeSaving, setRouteSaving] = useState(false);
  const [routeConfirmOpen, setRouteConfirmOpen] = useState(false);

  // Bindings dialog
  const [bindingModalOpen, setBindingModalOpen] = useState(false);
  const [currentRule, setCurrentRule] = useState<RouteRule | null>(null);
  const [bindingRows, setBindingRows] = useState<RouteBinding[]>([]);
  const [bindingLoading, setBindingLoading] = useState(false);

  // Binding form modal
  const [bindingFormOpen, setBindingFormOpen] = useState(false);
  const [editingBinding, setEditingBinding] = useState<RouteBinding | null>(
    null,
  );
  const [bindingForm, setBindingForm] = useState<BindingFormState>(() =>
    DEFAULT_BINDING_FORM(1),
  );
  const [bindingErrors, setBindingErrors] = useState<Record<string, string>>(
    {},
  );
  const [bindingSaving, setBindingSaving] = useState(false);
  const [bindingConfirmOpen, setBindingConfirmOpen] = useState(false);

  // Emergency switch modal
  const [switchOpen, setSwitchOpen] = useState(false);
  const [switchChannelId, setSwitchChannelId] = useState("");
  const [switchReason, setSwitchReason] = useState("");
  const [switching, setSwitching] = useState(false);

  // Import & delete confirms
  const [importOpen, setImportOpen] = useState(false);
  const [deleteRouteTarget, setDeleteRouteTarget] = useState<RouteRule | null>(
    null,
  );
  const [batchDeleteIds, setBatchDeleteIds] = useState<string[] | null>(null);
  const [deleteBindingTarget, setDeleteBindingTarget] =
    useState<RouteBinding | null>(null);
  const [deleting, setDeleting] = useState(false);

  const productMap = useMemo(
    () => new Map(productRows.map((p) => [p.id, p])),
    [productRows],
  );
  const channelMap = useMemo(
    () => new Map(channelRows.map((c) => [c.id, c])),
    [channelRows],
  );
  const tenantMap = useMemo(() => {
    const map = new Map<string, TenantRouteOption>();
    for (const item of authorizations) {
      if (!map.has(item.tenant_id)) {
        map.set(item.tenant_id, {
          tenant_id: item.tenant_id,
          tenant_code: item.tenant_code,
          tenant_name: item.tenant_name,
          product_id: item.product_id,
          product_code: item.product_code,
        });
      }
    }
    return map;
  }, [authorizations]);

  const tenantRouteOptions = useMemo(() => {
    const options: TenantRouteOption[] = authorizations.map((item) => ({
      tenant_id: item.tenant_id,
      tenant_code: item.tenant_code,
      tenant_name: item.tenant_name,
      product_id: item.product_id,
      product_code: item.product_code,
    }));
    if (!routeForm.product_id) return dedupeTenants(options);
    const matched = options.filter(
      (item) => item.product_id === routeForm.product_id,
    );
    return dedupeTenants(matched.length ? matched : options);
  }, [authorizations, routeForm.product_id]);

  const routePurposeOptions = useMemo(() => getRoutePurposeOptions(), [t]);
  const failureTypeOptions = useMemo(() => getFailureTypeOptions(), [t]);

  const channelName = useCallback(
    (channelId: string) => {
      const ch = channelMap.get(channelId);
      return ch
        ? `${ch.supplier_name} / ${ch.name} (${ch.channel_code})`
        : channelId || EMPTY_PLACEHOLDER;
    },
    [channelMap],
  );

  const productName = useCallback(
    (productId: string) => {
      const prod = productMap.get(productId);
      return prod
        ? `${prod.name} (${prod.product_id})`
        : productId || EMPTY_PLACEHOLDER;
    },
    [productMap],
  );

  const tenantName = useCallback(
    (tenantId?: string | null) => {
      if (!tenantId) return t("routes.labels.defaultRoute");
      const tenant = tenantMap.get(tenantId);
      return tenant ? `${tenant.tenant_name} (${tenant.tenant_code})` : tenantId;
    },
    [tenantMap, t],
  );

  const routeProductName = useCallback(
    (row: RouteRule) => {
      const prod = productMap.get(row.product_id);
      if (prod) return `${prod.name} (${prod.product_id})`;
      const routeConfig = getRecord(row.metadata?.route_config);
      const pName = getString(routeConfig.product_name);
      const pCode = getString(routeConfig.product_code, row.product_id);
      return pName ? `${pName} (${pCode})` : row.product_id || EMPTY_PLACEHOLDER;
    },
    [productMap],
  );

  const routeTenantName = useCallback(
    (row: RouteRule) => {
      if (!row.tenant_id) return t("routes.labels.defaultRoute");
      const tenant = tenantMap.get(row.tenant_id);
      if (tenant) return `${tenant.tenant_name} (${tenant.tenant_code})`;
      const routeConfig = getRecord(row.metadata?.route_config);
      const tName = getString(routeConfig.tenant_name);
      const tCode = getString(routeConfig.tenant_code, row.tenant_id);
      return tName ? `${tName} (${tCode})` : row.tenant_id;
    },
    [tenantMap, t],
  );

  const loadChannels = useCallback(async () => {
    const res = await fetchChannels({ page: 1, page_size: 100 });
    setChannelRows(res.channels ?? []);
  }, []);

  const loadReferenceData = useCallback(async () => {
    const [productsRes, authRes] = await Promise.allSettled([
      fetchProductList({ page: 1, page_size: 100 }),
      fetchAuthorizations({ page: 1, page_size: 100 }),
    ]);
    if (productsRes.status === "fulfilled") {
      setProductRows(productsRes.value.products ?? []);
    }
    if (authRes.status === "fulfilled") {
      setAuthorizations(authRes.value.authorizations ?? []);
    }
  }, []);

  const loadData = useCallback(async () => {
    if (lockProductProp && !initialProductId) {
      setTableData([]);
      setTotal(0);
      return;
    }
    setLoading(true);
    try {
      const filters: ListFilter[] = [];
      if (statusFilter) {
        filters.push({ field: "status", op: "eq", value: statusFilter });
      }
      const [routes] = await Promise.all([
        fetchRouteRules({
          page,
          page_size: pageSize,
          q: appliedKeyword || undefined,
          filters: filters.length ? JSON.stringify(filters) : undefined,
          product_id: initialProductId || undefined,
        }),
        loadChannels(),
        loadReferenceData(),
      ]);
      setTableData(routes.route_rules ?? []);
      setTotal(routes.pagination?.total ?? 0);
      setSelectedIds([]);
    } catch (error) {
      showErrorToast(
        getUserFacingErrorMessage(error, t("routes.messages.fetchRoutesFailed")),
      );
    } finally {
      setLoading(false);
    }
  }, [
    lockProductProp,
    initialProductId,
    page,
    pageSize,
    appliedKeyword,
    statusFilter,
    loadChannels,
    loadReferenceData,
    t,
  ]);

  useEffect(() => {
    void loadData();
  }, [loadData]);

  const refreshBindings = useCallback(
    async (ruleId: string) => {
      setBindingLoading(true);
      try {
        const res = await fetchRouteBindings(ruleId);
        setBindingRows(res.bindings ?? []);
      } catch (error) {
        showErrorToast(
          getUserFacingErrorMessage(
            error,
            t("routes.messages.fetchBindingsFailed"),
          ),
        );
      } finally {
        setBindingLoading(false);
      }
    },
    [t],
  );

  const openBindings = async (row: RouteRule) => {
    setCurrentRule(row);
    setBindingModalOpen(true);
    await refreshBindings(row.id);
  };

  const openCreateRoute = async () => {
    setEditingRoute(null);
    setRouteForm(DEFAULT_ROUTE_FORM(initialProductId));
    setRouteErrors({});
    await loadReferenceData();
    setRouteModalOpen(true);
  };

  const openEditRoute = async (row: RouteRule) => {
    if (isExampleRoute(row)) {
      showInfoToast(t("routes.messages.exampleNotEditable"));
      return;
    }
    setEditingRoute(row);
    await loadReferenceData();
    const rc = getRecord(row.metadata?.route_config);
    const mode =
      getString(rc.effective_mode) === "window" ? "window" : "immediate";
    const startAt = getString(rc.effective_start_at);
    const endAt = getString(rc.effective_end_at);
    setRouteForm({
      scope: row.tenant_id ? "tenant" : "default",
      tenant_id: row.tenant_id || "",
      product_id: row.product_id,
      priority: row.priority,
      status: row.status,
      route_purpose: getString(rc.purpose, "standard"),
      effective_mode: mode,
      effective_range:
        mode === "window" && startAt && endAt ? [startAt, endAt] : [],
      channel_id: "",
      timeout_ms: DEFAULT_TIMEOUT_MS,
      max_retries: DEFAULT_MAX_RETRIES,
    });
    setRouteErrors({});
    setRouteModalOpen(true);
  };

  const validateRouteForm = (): boolean => {
    const next: Record<string, string> = {};
    if (!routeForm.product_id) {
      next.product_id = t("routes.validation.productRequired");
    }
    if (routeForm.scope === "tenant" && !routeForm.tenant_id) {
      next.tenant_id = t("routes.validation.tenantRequired");
    }
    if (!Number.isFinite(routeForm.priority)) {
      next.priority = t("routes.validation.priorityRequired");
    }
    if (!routeForm.status) {
      next.status = t("routes.validation.statusRequired");
    }
    if (!routeForm.route_purpose) {
      next.route_purpose = t("routes.validation.routePurposeRequired");
    }
    if (
      routeForm.effective_mode === "window" &&
      (routeForm.effective_range.length !== 2 ||
        !routeForm.effective_range[0] ||
        !routeForm.effective_range[1])
    ) {
      next.effective_range = t("routes.validation.effectiveWindowRequired");
    }
    if (!editingRoute && !routeForm.channel_id) {
      next.channel_id = t("routes.validation.bindingChannelRequired");
    }
    setRouteErrors(next);
    return Object.keys(next).length === 0;
  };

  const handleRouteFormSubmit = () => {
    if (!validateRouteForm()) return;
    setRouteConfirmOpen(true);
  };

  const buildRouteMetadata = () => {
    const base = editingRoute?.metadata ? { ...editingRoute.metadata } : {};
    const [startAt, endAt] =
      routeForm.effective_mode === "window" &&
      routeForm.effective_range.length === 2
        ? routeForm.effective_range
        : ["", ""];
    const prod = productMap.get(routeForm.product_id);
    const tenant = routeForm.tenant_id
      ? tenantMap.get(routeForm.tenant_id)
      : undefined;
    return {
      ...base,
      route_config: {
        purpose: routeForm.route_purpose,
        purpose_label: routePurposeLabel(routeForm.route_purpose),
        effective_mode: routeForm.effective_mode,
        effective_start_at: startAt || null,
        effective_end_at: endAt || null,
        product_name: prod?.name || null,
        product_code: prod?.product_id || routeForm.product_id,
        tenant_name: tenant?.tenant_name || null,
        tenant_code: tenant?.tenant_code || null,
        updated_at: new Date().toISOString(),
      },
    };
  };

  const routeConfirmText = useMemo(() => {
    const lines = [
      `${t("routes.confirm.scope")}${routeForm.scope === "tenant" ? t("routes.scope.tenant") : t("routes.scope.default")}`,
      `${t("routes.confirm.product")}${productName(routeForm.product_id)}`,
      routeForm.scope === "tenant"
        ? `${t("routes.confirm.tenant")}${tenantName(routeForm.tenant_id)}`
        : "",
      `${t("routes.confirm.priority")}${routeForm.priority}`,
      `${t("routes.confirm.status")}${routeStatusLabel(routeForm.status)}`,
      `${t("routes.confirm.routePurpose")}${routePurposeLabel(routeForm.route_purpose)}`,
      routeForm.effective_mode === "window" &&
      routeForm.effective_range.length === 2
        ? `${t("routes.confirm.effectiveWindow")}${formatDateTime(routeForm.effective_range[0])} ${t("common.to")} ${formatDateTime(routeForm.effective_range[1])}`
        : t("routes.confirm.effectiveNow"),
    ].filter(Boolean);
    return lines.join("\n");
  }, [routeForm, productName, tenantName, t]);

  const executeSaveRoute = async () => {
    if (routeSaving) return;
    setRouteSaving(true);
    try {
      const metadata = buildRouteMetadata();
      if (editingRoute) {
        await updateRouteRule(editingRoute.id, {
          priority: routeForm.priority,
          status: routeForm.status,
          metadata,
        });
      } else {
        await createRouteRule({
          tenant_id: routeForm.scope === "tenant" ? routeForm.tenant_id : null,
          product_id: routeForm.product_id,
          priority: routeForm.priority,
          status: routeForm.status,
          metadata,
          channel_id: routeForm.channel_id,
          timeout_ms: routeForm.timeout_ms,
          max_retries: routeForm.max_retries,
        });
      }
      showSuccessToast(t("common.feedback.saveSuccess"));
      setRouteConfirmOpen(false);
      setRouteModalOpen(false);
      await loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        showErrorToast(
          getUserFacingErrorMessage(error, t("routes.messages.saveRouteFailed")),
        );
      }
    } finally {
      setRouteSaving(false);
    }
  };

  const handleConfirmDeleteRoute = async () => {
    if (!deleteRouteTarget || deleting) return;
    setDeleting(true);
    try {
      await deleteRouteRule(deleteRouteTarget.id);
      showSuccessToast(t("common.feedback.deleteSuccess"));
      setDeleteRouteTarget(null);
      await loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        showErrorToast(
          getUserFacingErrorMessage(
            error,
            t("routes.messages.deleteRouteFailed"),
          ),
        );
      }
    } finally {
      setDeleting(false);
    }
  };

  const handleConfirmBatchDelete = async () => {
    if (!batchDeleteIds || batchDeleteIds.length === 0 || deleting) return;
    const deletable = tableData.filter(
      (r) => batchDeleteIds.includes(r.id) && !isExampleRoute(r),
    );
    if (deletable.length === 0) {
      showInfoToast(t("routes.messages.exampleNotDeletable"));
      setBatchDeleteIds(null);
      return;
    }
    setDeleting(true);
    try {
      for (const row of deletable) {
        await deleteRouteRule(row.id);
      }
      showSuccessToast(t("common.feedback.deleteSuccess"));
      setBatchDeleteIds(null);
      setSelectedIds([]);
      await loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        showErrorToast(
          getUserFacingErrorMessage(
            error,
            t("routes.messages.deleteRouteFailed"),
          ),
        );
      }
    } finally {
      setDeleting(false);
    }
  };

  // Binding operations
  const isCurrentRuleExample = Boolean(
    currentRule && isExampleRoute(currentRule),
  );
  const currentRuleTitle = currentRule
    ? `${routeTenantName(currentRule)} / ${routeProductName(currentRule)}`
    : EMPTY_PLACEHOLDER;

  const nextBindingOrder = () =>
    bindingRows.reduce((max, row) => Math.max(max, row.channel_order), 0) + 1;

  const openCreateBinding = async () => {
    if (!currentRule) return;
    if (isExampleRoute(currentRule)) {
      showInfoToast(t("routes.messages.exampleNotAddBinding"));
      return;
    }
    setEditingBinding(null);
    setBindingForm(DEFAULT_BINDING_FORM(nextBindingOrder()));
    setBindingErrors({});
    await loadChannels();
    setBindingFormOpen(true);
  };

  const openEditBinding = async (row: RouteBinding) => {
    if (isCurrentRuleExample) {
      showInfoToast(t("routes.messages.exampleNotEditable"));
      return;
    }
    setEditingBinding(row);
    await loadChannels();
    const sc = row.switch_condition || {};
    const parsedTypes = getStringArray(sc.failure_types);
    setBindingForm({
      channel_id: row.channel_id,
      channel_order: row.channel_order,
      timeout_ms: getNumber(sc.timeout_ms, DEFAULT_TIMEOUT_MS),
      max_retries: getNumber(sc.max_retries, DEFAULT_MAX_RETRIES),
      failover_enabled: sc.failover_enabled !== false,
      failure_types: parsedTypes.length
        ? parsedTypes
        : [...DEFAULT_FAILURE_TYPES],
      description: getString(sc.description),
    });
    setBindingErrors({});
    setBindingFormOpen(true);
  };

  const validateBindingForm = (): boolean => {
    const next: Record<string, string> = {};
    if (!bindingForm.channel_id) {
      next.channel_id = t("routes.validation.bindingChannelRequired");
    }
    if (!Number.isFinite(bindingForm.channel_order) || bindingForm.channel_order < 1) {
      next.channel_order = t("routes.validation.channelOrderRequired");
    }
    if (!Number.isFinite(bindingForm.timeout_ms) || bindingForm.timeout_ms < 100) {
      next.timeout_ms = t("routes.validation.timeoutRequired");
    }
    if (!Number.isFinite(bindingForm.max_retries) || bindingForm.max_retries < 0) {
      next.max_retries = t("routes.validation.maxRetriesRequired");
    }
    setBindingErrors(next);
    return Object.keys(next).length === 0;
  };

  const handleBindingFormSubmit = () => {
    if (!currentRule) return;
    if (isCurrentRuleExample) {
      showInfoToast(t("routes.messages.exampleNotSaveBinding"));
      return;
    }
    if (!validateBindingForm()) return;
    setBindingConfirmOpen(true);
  };

  const bindingConfirmText = useMemo(() => {
    const lines = [
      `${t("routes.confirm.route")}${currentRuleTitle}`,
      `${t("routes.confirm.upstreamChannel")}${channelName(bindingForm.channel_id)}`,
      `${t("routes.confirm.channelOrder")}${bindingForm.channel_order}`,
      `${t("routes.confirm.timeout")}${bindingForm.timeout_ms} ${t("routes.units.ms")}`,
      `${t("routes.confirm.maxRetries")}${bindingForm.max_retries}`,
      `${t("routes.confirm.failover")}${bindingForm.failover_enabled ? t("routes.failover.activeText") : t("routes.failover.inactiveText")}`,
      bindingForm.failover_enabled
        ? `${t("routes.confirm.failureTypes")}${bindingForm.failure_types.map(failureTypeLabel).join("、") || t("routes.labels.unselected")}`
        : "",
      bindingForm.description.trim()
        ? `${t("routes.confirm.description")}${bindingForm.description.trim()}`
        : "",
    ].filter(Boolean);
    return lines.join("\n");
  }, [bindingForm, currentRuleTitle, channelName, t]);

  const executeSaveBinding = async () => {
    if (!currentRule || bindingSaving) return;
    setBindingSaving(true);
    try {
      const base = editingBinding?.switch_condition
        ? { ...editingBinding.switch_condition }
        : {};
      const switchCondition = {
        ...base,
        max_retries: bindingForm.max_retries,
        timeout_ms: bindingForm.timeout_ms,
        failover_enabled: bindingForm.failover_enabled,
        failure_types: bindingForm.failover_enabled
          ? bindingForm.failure_types
          : [],
        description: bindingForm.description.trim() || null,
      };
      if (editingBinding) {
        await updateRouteBinding(editingBinding.id, {
          channel_id: bindingForm.channel_id,
          channel_order: bindingForm.channel_order,
          switch_condition: switchCondition,
        });
      } else {
        await createRouteBinding(currentRule.id, {
          channel_id: bindingForm.channel_id,
          channel_order: bindingForm.channel_order,
          switch_condition: switchCondition,
        });
      }
      showSuccessToast(t("common.feedback.saveSuccess"));
      setBindingConfirmOpen(false);
      setBindingFormOpen(false);
      await refreshBindings(currentRule.id);
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        showErrorToast(
          getUserFacingErrorMessage(
            error,
            t("routes.messages.saveBindingFailed"),
          ),
        );
      }
    } finally {
      setBindingSaving(false);
    }
  };

  const handleConfirmDeleteBinding = async () => {
    if (!deleteBindingTarget || !currentRule || deleting) return;
    setDeleting(true);
    try {
      await deleteRouteBinding(deleteBindingTarget.id);
      showSuccessToast(t("common.feedback.deleteSuccess"));
      setDeleteBindingTarget(null);
      await refreshBindings(currentRule.id);
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        showErrorToast(
          getUserFacingErrorMessage(
            error,
            t("routes.messages.deleteBindingFailed"),
          ),
        );
      }
    } finally {
      setDeleting(false);
    }
  };

  const openEmergencySwitch = (row: RouteBinding) => {
    if (isCurrentRuleExample) {
      showInfoToast(t("routes.messages.exampleNotSwitch"));
      return;
    }
    setSwitchChannelId(row.channel_id);
    setSwitchReason("");
    setSwitchOpen(true);
  };

  const handleSubmitEmergencySwitch = async () => {
    if (!currentRule || switching) return;
    if (isCurrentRuleExample) {
      showInfoToast(t("routes.messages.exampleNotSwitch"));
      return;
    }
    if (!switchReason.trim()) {
      showWarningToast(t("routes.validation.switchReasonRequired"));
      return;
    }
    setSwitching(true);
    try {
      await emergencySwitchRoute(currentRule.id, {
        channel_id: switchChannelId,
        reason: switchReason.trim(),
      });
      showSuccessToast(t("routes.messages.emergencySwitchSuccess"));
      setSwitchOpen(false);
      await refreshBindings(currentRule.id);
      await loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        showErrorToast(
          getUserFacingErrorMessage(
            error,
            t("routes.messages.emergencySwitchFailed"),
          ),
        );
      }
    } finally {
      setSwitching(false);
    }
  };

  // CSV Import & Export
  const routeImportColumns = useMemo<ColumnConfig[]>(
    () => [
      {
        key: "product_code",
        label: t("routes.import.columns.productCode"),
        required: true,
      },
      {
        key: "tenant_code",
        label: t("routes.import.columns.tenantCode"),
        required: true,
      },
      {
        key: "channel_code",
        label: t("channels.fields.channelCode"),
        required: true,
      },
      {
        key: "priority",
        label: t("routes.import.columns.priority"),
        required: false,
        type: "number",
      },
      {
        key: "status",
        label: t("routes.import.columns.status"),
        required: false,
      },
      {
        key: "route_purpose",
        label: t("routes.import.columns.routePurpose"),
        required: false,
      },
    ],
    [t],
  );

  const routeImportFormatHint = useMemo(
    () =>
      [
        t("routes.import.formatHintHeader"),
        t("routes.import.formatHintExample"),
      ].join("\n"),
    [t],
  );

  const routeImportExistingData = useMemo<Record<string, unknown>[]>(
    () =>
      tableData.map((row) => {
        const rc = getRecord(row.metadata?.route_config);
        return {
          key: `${getString(rc.product_code, row.product_id)}/${getString(rc.tenant_code, row.tenant_id || "")}`,
        };
      }),
    [tableData],
  );

  const validateRouteImportRow = (row: Record<string, string>) => {
    if (!row.product_code?.trim()) {
      return {
        valid: false,
        reason: t("routes.import.validation.productCodeRequired"),
      };
    }
    if (!row.tenant_code?.trim()) {
      return {
        valid: false,
        reason: t("routes.import.validation.tenantCodeRequired"),
      };
    }
    if (!row.channel_code?.trim()) {
      return {
        valid: false,
        reason: t("channels.validation.channelCodeRequired"),
      };
    }
    if (row.priority?.trim() && Number.isNaN(Number(row.priority))) {
      return {
        valid: false,
        reason: t("routes.import.validation.priorityNumeric"),
      };
    }
    if (
      row.status?.trim() &&
      !["active", "inactive", "suspended"].includes(row.status.trim())
    ) {
      return {
        valid: false,
        reason: t("routes.import.validation.statusInvalid"),
      };
    }
    if (
      row.route_purpose?.trim() &&
      !routePurposeOptions.some(
        (item) => item.value === row.route_purpose.trim(),
      )
    ) {
      return {
        valid: false,
        reason: t("routes.import.validation.routePurposeInvalid"),
      };
    }
    return { valid: true };
  };

  const transformRouteImportRow = (
    row: Record<string, string>,
  ): RouteImportRow => {
    const productCode = row.product_code.trim();
    const tenantCode = row.tenant_code.trim();
    const channelCode = row.channel_code.trim();
    return {
      key: `${productCode}/${tenantCode}`,
      product_code: productCode,
      tenant_code: tenantCode,
      channel_code: channelCode,
      priority: Number(row.priority?.trim() || 100),
      status: row.status?.trim() || "active",
      route_purpose: row.route_purpose?.trim() || "standard",
    };
  };

  const handleRouteImport = async (items: Record<string, unknown>[]) => {
    const productByCode = new Map(
      productRows.map((product) => [product.product_id, product]),
    );
    const authorizationByKey = new Map(
      authorizations.map((item) => [
        `${item.product_code}/${item.tenant_code}`,
        item,
      ]),
    );
    const channelByCode = new Map(
      channelRows.map((ch) => [ch.channel_code, ch]),
    );
    await runResourceCsvImport<RouteImportRow>(
      items,
      async (item) => {
        const product = productByCode.get(item.product_code);
        const auth = authorizationByKey.get(
          `${item.product_code}/${item.tenant_code}`,
        );
        const channel = channelByCode.get(item.channel_code);
        if (!product || !auth) {
          throw new Error(
            `Unknown authorized tenant route: ${item.product_code} / ${item.tenant_code}`,
          );
        }
        if (!channel) {
          throw new Error(`Unknown channel code: ${item.channel_code}`);
        }
        const purpose = item.route_purpose || "standard";
        await createRouteRule({
          tenant_id: auth.tenant_id,
          product_id: product.id,
          priority: item.priority || 100,
          status: item.status || "active",
          metadata: {
            route_config: {
              purpose,
              purpose_label: routePurposeLabel(purpose),
              effective_mode: "immediate",
              effective_start_at: null,
              effective_end_at: null,
              product_name: product.name,
              product_code: product.product_id,
              tenant_name: auth.tenant_name,
              tenant_code: auth.tenant_code,
              updated_at: new Date().toISOString(),
            },
          },
          channel_id: channel.id,
        });
      },
      {
        onError: (error) => {
          logger.error("Failed to import route:", error);
        },
      },
    );
  };

  const handleExportCommand = (command: string) => {
    const rows =
      command === "selected"
        ? tableData.filter((r) => selectedIds.includes(r.id))
        : tableData;
    const csvRows = rows.map((row) => {
      const rc = getRecord(row.metadata?.route_config);
      return {
        tenant: routeTenantName(row),
        product: routeProductName(row),
        priority: row.priority,
        status: routeStatusLabel(row.status),
        route_purpose:
          routePurposeLabelFromMetadata(row.metadata) ||
          routePurposeLabel(getString(rc.purpose)),
        created_at: row.created_at,
      };
    });
    exportResourceCsv(
      csvRows,
      [
        { key: "tenant", label: t("routes.columns.tenant") },
        { key: "product", label: t("routes.columns.product") },
        { key: "priority", label: t("routes.columns.priority") },
        { key: "status", label: t("routes.columns.status") },
        { key: "route_purpose", label: t("routes.fields.routePurpose") },
        { key: "created_at", label: t("common.fields.createdAt") },
      ],
      `user-routes-${command}-${Date.now().toString(36)}.csv`,
    );
  };

  const exportItems = useMemo(
    () => [
      {
        command: "page",
        label: t("routes.actions.exportPage"),
        disabled: tableData.length === 0,
      },
      {
        command: "selected",
        label: t("routes.actions.exportSelected", {
          count: selectedIds.length,
        }),
        disabled: selectedIds.length === 0,
      },
    ],
    [t, tableData.length, selectedIds.length],
  );

  const channelSelectOptions = useMemo<SelectOptionItem[]>(
    () =>
      channelRows.map((ch) => ({
        value: ch.id,
        text: `${ch.supplier_name} ${ch.name} ${ch.channel_code}`,
        label: (
          <span className="inline-flex items-center gap-1.5 min-w-0">
            <span className="truncate">
              {ch.supplier_name} / {ch.name} ({ch.channel_code})
            </span>
            {ch.status !== "active" && (
              <Badge variant="neutral">{channelStatusLabel(ch.status)}</Badge>
            )}
          </span>
        ),
      })),
    [channelRows],
  );

  const productSelectOptions = useMemo<SelectOptionItem[]>(
    () =>
      productRows.map((p) => ({
        value: p.id,
        label: `${p.name} (${p.product_id})`,
        text: `${p.name} ${p.product_id}`,
      })),
    [productRows],
  );

  const tenantSelectOptions = useMemo<SelectOptionItem[]>(
    () =>
      tenantRouteOptions.map((item) => ({
        value: item.tenant_id,
        label: `${item.tenant_name} (${item.tenant_code})`,
        text: `${item.tenant_name} ${item.tenant_code}`,
      })),
    [tenantRouteOptions],
  );

  const columns = useMemo<DataTableColumn<RouteRule>[]>(() => {
    const cols: DataTableColumn<RouteRule>[] = [
      {
        id: "tenant_id",
        header: t("routes.columns.tenant"),
        minWidth: 160,
        cell: (row) => (
          <span className="font-medium text-foreground">
            {routeTenantName(row)}
          </span>
        ),
      },
    ];
    if (!lockProduct) {
      cols.push({
        id: "product_id",
        header: t("routes.columns.product"),
        minWidth: 160,
        cell: (row) => routeProductName(row),
      });
    }
    cols.push(
      {
        id: "priority",
        header: t("routes.columns.priority"),
        width: 90,
        align: "right",
        mono: true,
        accessorKey: "priority",
      },
      {
        id: "status",
        header: t("routes.columns.status"),
        width: 100,
        cell: (row) => (
          <StatusBadge
            status={routeStatusBadgeKey(row.status)}
            label={routeStatusLabel(row.status)}
          />
        ),
      },
      {
        id: "metadata",
        header: t("routes.columns.metadata"),
        minWidth: 160,
        cell: (row) => {
          const emergency = isEmergencySwitchEnabled(row.metadata);
          const example = isExampleRoute(row);
          const purpose = routePurposeLabelFromMetadata(row.metadata);
          const hasMeta = hasMetadata(row.metadata);
          if (!hasMeta) {
            return <span className="text-muted-foreground">{EMPTY_PLACEHOLDER}</span>;
          }
          return (
            <div className="flex flex-wrap items-center gap-1">
              {emergency && (
                <Badge variant="danger">{t("routes.labels.emergencySwitch")}</Badge>
              )}
              {example && (
                <Badge variant="warning">{t("routes.labels.example")}</Badge>
              )}
              {purpose && <Badge variant="neutral">{purpose}</Badge>}
              {!emergency && !example && !purpose && (
                <Badge variant="neutral">{t("routes.labels.configured")}</Badge>
              )}
            </div>
          );
        },
      },
      {
        id: "created_at",
        header: t("common.fields.createdAt"),
        width: 168,
        mono: true,
        cell: (row) => formatDateTime(row.created_at),
      },
      {
        id: "operations",
        header: t("common.fields.operations"),
        width: canWrite ? 210 : 100,
        align: "right",
        cell: (row) => {
          const example = isExampleRoute(row);
          return (
            <div className="inline-flex items-center justify-end gap-1">
              <Button
                type="button"
                variant="link"
                size="xs"
                onClick={() => void openBindings(row)}
              >
                {t("routes.actions.bindings")}
              </Button>
              {canWrite && (
                <>
                  <Button
                    type="button"
                    variant="link"
                    size="xs"
                    disabled={example}
                    onClick={() => void openEditRoute(row)}
                  >
                    {t("common.actions.modify")}
                  </Button>
                  <Button
                    type="button"
                    variant="link"
                    size="xs"
                    disabled={example}
                    className="text-danger-fg hover:text-danger-fg hover:bg-danger-fg/10"
                    onClick={() => setDeleteRouteTarget(row)}
                  >
                    {t("common.actions.delete")}
                  </Button>
                </>
              )}
            </div>
          );
        },
      },
    );
    return cols;
  }, [
    t,
    lockProduct,
    canWrite,
    routeTenantName,
    routeProductName,
  ]);

  const handleSearch = () => {
    setPage(1);
    setAppliedKeyword(keywordInput.trim());
  };

  return (
    <div data-testid="product-routes-section">
      <DataTablePage
        title={!embedded ? t("menus.mazbri.advancedRouteConfig") : undefined}
        columns={columns}
        data={tableData}
        getRowId={(row) => row.id}
        loading={loading}
        selectable={canWrite}
        selectedKeys={selectedIds}
        onSelectionChange={setSelectedIds}
        page={page}
        pageSize={pageSize}
        total={total}
        pageSizeOptions={[10, 20, 50]}
        onPageChange={setPage}
        onPageSizeChange={(next) => {
          setPageSize(next);
          setPage(1);
        }}
        className={
          borderless
            ? "[&>div]:border-0 [&>div]:rounded-none [&>div]:shadow-none"
            : undefined
        }
        filters={
          <>
            <div className="flex items-center gap-1.5">
              <Input
                value={keywordInput}
                onChange={(e) => setKeywordInput(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleSearch();
                }}
                placeholder={t("common.search")}
                className="w-56"
              />
              <Button
                type="button"
                variant="outline"
                size="md"
                onClick={handleSearch}
              >
                <Search className="h-3.5 w-3.5" />
                {t("common.fields.search")}
              </Button>
            </div>
            <Select
              value={statusFilter}
              onChange={(e) => {
                setStatusFilter(e.target.value);
                setPage(1);
              }}
              aria-label={t("routes.columns.status")}
              className="w-32"
              options={[
                { value: "", label: t("common.all"), text: t("common.all") },
                {
                  value: "active",
                  label: t("routes.status.active"),
                  text: t("routes.status.active"),
                },
                {
                  value: "inactive",
                  label: t("routes.status.inactive"),
                  text: t("routes.status.inactive"),
                },
                {
                  value: "suspended",
                  label: t("routes.status.suspended"),
                  text: t("routes.status.suspended"),
                },
              ]}
            />
          </>
        }
        batchActions={
          canWrite
            ? (ids) => (
                <Button
                  type="button"
                  variant="danger"
                  size="md"
                  onClick={() => setBatchDeleteIds(ids)}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                  {t("common.actions.delete")}
                </Button>
              )
            : undefined
        }
        actions={
          <>
            <Button
              type="button"
              variant="outline"
              size="md"
              onClick={() => void loadData()}
            >
              <RefreshCw className="h-3.5 w-3.5" />
              {t("common.actions.refresh")}
            </Button>
            {canWrite && (
              <Button
                type="button"
                variant="outline"
                size="md"
                onClick={async () => {
                  await loadReferenceData();
                  setImportOpen(true);
                }}
              >
                <Upload className="h-3.5 w-3.5" />
                {t("common.actions.import")}
              </Button>
            )}
            <ExportDropdown
              items={exportItems}
              disabled={tableData.length === 0}
              onCommand={handleExportCommand}
            />
            {canWrite && (
              <Button
                type="button"
                variant="primary"
                size="md"
                onClick={() => void openCreateRoute()}
                data-testid="create-route-btn"
              >
                <Plus className="h-3.5 w-3.5" />
                {lockProduct
                  ? t("routes.actions.createProductRoute")
                  : t("routes.actions.createRoute")}
              </Button>
            )}
          </>
        }
      />

      {/* Upstream Bindings Modal */}
      <Modal
        open={bindingModalOpen}
        onClose={() => setBindingModalOpen(false)}
        title={t("routes.dialogs.bindingTitle")}
        subtitle={currentRuleTitle}
        size="lg"
        testId="route-bindings-modal"
      >
        <div className="flex items-center justify-between gap-2 pb-2">
          <span className="text-xs font-medium text-foreground">
            {currentRuleTitle}
          </span>
          {canWrite && (
            <Button
              type="button"
              variant="primary"
              size="sm"
              disabled={!currentRule || isCurrentRuleExample}
              onClick={() => void openCreateBinding()}
            >
              <Plus className="h-3.5 w-3.5" />
              {t("routes.actions.addUpstreamBinding")}
            </Button>
          )}
        </div>

        <div className="overflow-x-auto rounded-lg border border-border">
          <table className="w-full border-collapse text-xs">
            <thead>
              <tr className="border-b border-border bg-muted/45 text-muted-foreground">
                <th className="h-9 px-3 text-left font-medium w-16">
                  {t("routes.bindingColumns.order")}
                </th>
                <th className="h-9 px-3 text-left font-medium">
                  {t("routes.bindingColumns.channel")}
                </th>
                <th className="h-9 px-3 text-left font-medium">
                  {t("routes.bindingColumns.channelId")}
                </th>
                <th className="h-9 px-3 text-left font-medium">
                  {t("routes.bindingColumns.switchCondition")}
                </th>
                {canWrite && (
                  <th className="h-9 px-3 text-right font-medium w-52">
                    {t("common.fields.operations")}
                  </th>
                )}
              </tr>
            </thead>
            <tbody className="divide-y divide-border-subtle">
              {bindingLoading ? (
                <tr>
                  <td
                    colSpan={canWrite ? 5 : 4}
                    className="py-8 text-center text-muted-foreground"
                  >
                    {t("layout.loading")}
                  </td>
                </tr>
              ) : bindingRows.length === 0 ? (
                <tr>
                  <td
                    colSpan={canWrite ? 5 : 4}
                    className="py-8 text-center text-muted-foreground"
                  >
                    {t("common.feedback.noData")}
                  </td>
                </tr>
              ) : (
                bindingRows.map((row) => (
                  <tr key={row.id} className="hover:bg-hover-bg/70">
                    <td className="px-3 py-2 font-mono-tech tabular-nums">
                      #{row.channel_order}
                    </td>
                    <td className="px-3 py-2 font-medium text-foreground">
                      {channelName(row.channel_id)}
                    </td>
                    <td className="px-3 py-2">
                      <CopyableText
                        value={row.channel_id}
                        truncate
                        maxWidthClass="max-w-[140px]"
                      />
                    </td>
                    <td className="px-3 py-2 text-muted-foreground">
                      {switchConditionSummary(row.switch_condition)}
                    </td>
                    {canWrite && (
                      <td className="px-3 py-2 text-right">
                        <div className="inline-flex items-center justify-end gap-1">
                          <Button
                            type="button"
                            variant="link"
                            size="xs"
                            disabled={isCurrentRuleExample}
                            onClick={() => void openEditBinding(row)}
                          >
                            {t("common.actions.modify")}
                          </Button>
                          <Button
                            type="button"
                            variant="link"
                            size="xs"
                            disabled={isCurrentRuleExample}
                            className="text-danger-fg hover:text-danger-fg hover:bg-danger-fg/10"
                            onClick={() => setDeleteBindingTarget(row)}
                          >
                            {t("common.actions.delete")}
                          </Button>
                          <Button
                            type="button"
                            variant="link"
                            size="xs"
                            disabled={isCurrentRuleExample}
                            className="text-danger-fg hover:text-danger-fg hover:bg-danger-fg/10"
                            onClick={() => openEmergencySwitch(row)}
                          >
                            <Zap className="h-3 w-3" />
                            {t("routes.actions.emergencySwitch")}
                          </Button>
                        </div>
                      </td>
                    )}
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </Modal>

      {/* Create / Edit Route Modal */}
      <FormModal
        open={routeModalOpen}
        onClose={() => setRouteModalOpen(false)}
        onSubmit={handleRouteFormSubmit}
        title={
          editingRoute
            ? t("routes.dialogs.editRouteTitle")
            : t("routes.dialogs.createRouteTitle")
        }
        size="lg"
        submitLabel={t("common.actions.save")}
        cancelLabel={t("common.cancel")}
        submitting={routeSaving}
        testId="route-form-modal"
      >
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
          <FormField
            label={t("routes.fields.product")}
            required
            error={routeErrors.product_id}
          >
            <Select
              value={routeForm.product_id}
              options={productSelectOptions}
              searchable
              disabled={Boolean(editingRoute) || lockProduct}
              placeholder={t("routes.placeholders.product")}
              onChange={(e) =>
                setRouteForm((prev) => ({ ...prev, product_id: e.target.value }))
              }
              className="w-full"
            />
          </FormField>

          {routeForm.scope === "tenant" && (
            <FormField
              label={t("routes.fields.tenant")}
              required
              error={routeErrors.tenant_id}
            >
              <Select
                value={routeForm.tenant_id}
                options={tenantSelectOptions}
                searchable
                disabled={Boolean(editingRoute)}
                placeholder={t("routes.placeholders.tenant")}
                onChange={(e) =>
                  setRouteForm((prev) => ({
                    ...prev,
                    tenant_id: e.target.value,
                  }))
                }
                className="w-full"
              />
            </FormField>
          )}

          <FormField
            label={t("routes.fields.priority")}
            required
            error={routeErrors.priority}
          >
            <NumberInput
              value={routeForm.priority}
              min={0}
              step={1}
              onChange={(val) =>
                setRouteForm((prev) => ({ ...prev, priority: val }))
              }
            />
          </FormField>

          <FormField
            label={t("routes.fields.status")}
            required
            error={routeErrors.status}
          >
            <Select
              value={routeForm.status}
              onChange={(e) =>
                setRouteForm((prev) => ({ ...prev, status: e.target.value }))
              }
              className="w-full"
              options={[
                {
                  value: "active",
                  label: t("routes.status.active"),
                  text: t("routes.status.active"),
                },
                {
                  value: "inactive",
                  label: t("routes.status.inactive"),
                  text: t("routes.status.inactive"),
                },
                {
                  value: "suspended",
                  label: t("routes.status.suspended"),
                  text: t("routes.status.suspended"),
                },
              ]}
            />
          </FormField>

          <FormField
            label={t("routes.fields.routePurpose")}
            required
            error={routeErrors.route_purpose}
          >
            <Select
              value={routeForm.route_purpose}
              onChange={(e) =>
                setRouteForm((prev) => ({
                  ...prev,
                  route_purpose: e.target.value,
                }))
              }
              placeholder={t("routes.placeholders.routePurpose")}
              className="w-full"
              options={routePurposeOptions.map((opt) => ({
                value: opt.value,
                label: opt.label,
                text: opt.label,
              }))}
            />
          </FormField>
        </div>

        <FormField
          label={t("routes.fields.effectiveWindow")}
          error={routeErrors.effective_range}
        >
          <div className="space-y-2.5">
            <SegmentedControl
              value={routeForm.effective_mode}
              onChange={(next) =>
                setRouteForm((prev) => ({
                  ...prev,
                  effective_mode: next,
                  effective_range: next === "immediate" ? [] : prev.effective_range,
                }))
              }
              items={[
                {
                  key: "immediate",
                  label: t("routes.effectiveMode.immediate"),
                },
                {
                  key: "window",
                  label: t("routes.effectiveMode.window"),
                },
              ]}
            />
            {routeForm.effective_mode === "window" && (
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                <Input
                  type="datetime-local"
                  value={toDatetimeLocalValue(routeForm.effective_range[0] ?? "")}
                  onChange={(e) => {
                    const start = toIsoFromDatetimeLocal(e.target.value);
                    const end = routeForm.effective_range[1] ?? "";
                    setRouteForm((prev) => ({
                      ...prev,
                      effective_range: [start, end],
                    }));
                  }}
                  aria-label={t("common.startTime")}
                />
                <Input
                  type="datetime-local"
                  value={toDatetimeLocalValue(routeForm.effective_range[1] ?? "")}
                  onChange={(e) => {
                    const start = routeForm.effective_range[0] ?? "";
                    const end = toIsoFromDatetimeLocal(e.target.value);
                    setRouteForm((prev) => ({
                      ...prev,
                      effective_range: [start, end],
                    }));
                  }}
                  aria-label={t("common.endTime")}
                />
              </div>
            )}
          </div>
        </FormField>

        {!editingRoute && (
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 border-t border-border pt-4">
            <FormField
              label={t("routes.fields.upstreamChannel")}
              required
              error={routeErrors.channel_id}
              className="sm:col-span-2"
            >
              <Select
                value={routeForm.channel_id}
                options={channelSelectOptions}
                searchable
                placeholder={t("routes.placeholders.upstreamChannel")}
                onChange={(e) =>
                  setRouteForm((prev) => ({
                    ...prev,
                    channel_id: e.target.value,
                  }))
                }
                className="w-full"
              />
            </FormField>

            <FormField label={t("routes.fields.timeoutMs")}>
              <div className="flex items-center gap-2">
                <NumberInput
                  value={routeForm.timeout_ms}
                  min={100}
                  step={100}
                  onChange={(val) =>
                    setRouteForm((prev) => ({ ...prev, timeout_ms: val }))
                  }
                />
                <span className="text-xs text-muted-foreground shrink-0">
                  {t("routes.units.ms")}
                </span>
              </div>
            </FormField>

            <FormField label={t("routes.fields.maxRetries")}>
              <NumberInput
                value={routeForm.max_retries}
                min={0}
                step={1}
                onChange={(val) =>
                  setRouteForm((prev) => ({ ...prev, max_retries: val }))
                }
              />
            </FormField>
          </div>
        )}
      </FormModal>

      {/* Binding Create / Edit Form Modal */}
      <FormModal
        open={bindingFormOpen}
        onClose={() => setBindingFormOpen(false)}
        onSubmit={handleBindingFormSubmit}
        title={
          editingBinding
            ? t("routes.dialogs.editUpstreamBindingTitle")
            : t("routes.dialogs.createUpstreamBindingTitle")
        }
        size="md"
        submitLabel={t("common.actions.save")}
        cancelLabel={t("common.cancel")}
        submitting={bindingSaving}
        testId="binding-form-modal"
      >
        <FormField
          label={t("routes.fields.upstreamChannel")}
          required
          error={bindingErrors.channel_id}
        >
          <Select
            value={bindingForm.channel_id}
            options={channelSelectOptions}
            searchable
            placeholder={t("routes.placeholders.upstreamChannel")}
            onChange={(e) =>
              setBindingForm((prev) => ({
                ...prev,
                channel_id: e.target.value,
              }))
            }
            className="w-full"
          />
        </FormField>

        <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
          <FormField
            label={t("routes.fields.channelOrder")}
            required
            error={bindingErrors.channel_order}
          >
            <NumberInput
              value={bindingForm.channel_order}
              min={1}
              step={1}
              onChange={(val) =>
                setBindingForm((prev) => ({ ...prev, channel_order: val }))
              }
            />
          </FormField>

          <FormField
            label={t("routes.fields.timeoutMs")}
            required
            error={bindingErrors.timeout_ms}
          >
            <NumberInput
              value={bindingForm.timeout_ms}
              min={100}
              step={100}
              onChange={(val) =>
                setBindingForm((prev) => ({ ...prev, timeout_ms: val }))
              }
            />
          </FormField>

          <FormField
            label={t("routes.fields.maxRetries")}
            required
            error={bindingErrors.max_retries}
          >
            <NumberInput
              value={bindingForm.max_retries}
              min={0}
              step={1}
              onChange={(val) =>
                setBindingForm((prev) => ({ ...prev, max_retries: val }))
              }
            />
          </FormField>
        </div>

        <FormField label={t("routes.fields.failoverEnabled")}>
          <div className="flex items-center gap-2.5 h-8">
            <Switch
              checked={bindingForm.failover_enabled}
              onCheckedChange={(checked) =>
                setBindingForm((prev) => ({
                  ...prev,
                  failover_enabled: checked,
                }))
              }
              aria-label={t("routes.fields.failoverEnabled")}
            />
            <span className="text-xs text-muted-foreground">
              {bindingForm.failover_enabled
                ? t("routes.failover.activeText")
                : t("routes.failover.inactiveText")}
            </span>
          </div>
        </FormField>

        {bindingForm.failover_enabled && (
          <FormField label={t("routes.fields.failureTypes")}>
            <div className="flex flex-wrap gap-3 pt-1">
              {failureTypeOptions.map((opt) => {
                const checked = bindingForm.failure_types.includes(opt.value);
                return (
                  <label
                    key={opt.value}
                    className="inline-flex items-center gap-1.5 text-xs text-foreground cursor-pointer select-none"
                  >
                    <input
                      type="checkbox"
                      checked={checked}
                      onChange={(e) => {
                        const nextChecked = e.target.checked;
                        setBindingForm((prev) => ({
                          ...prev,
                          failure_types: nextChecked
                            ? [...prev.failure_types, opt.value]
                            : prev.failure_types.filter((v) => v !== opt.value),
                        }));
                      }}
                    />
                    <span>{opt.label}</span>
                  </label>
                );
              })}
            </div>
          </FormField>
        )}

        <FormField label={t("routes.fields.description")}>
          <Textarea
            rows={3}
            value={bindingForm.description}
            onChange={(e) =>
              setBindingForm((prev) => ({
                ...prev,
                description: e.target.value,
              }))
            }
            placeholder={t("routes.placeholders.description")}
          />
        </FormField>
      </FormModal>

      {/* Save Confirm Dialogs */}
      <ConfirmDialog
        open={routeConfirmOpen}
        onClose={() => setRouteConfirmOpen(false)}
        onConfirm={() => void executeSaveRoute()}
        title={t("routes.dialogs.confirmSaveRoute")}
        description={
          <pre className="whitespace-pre-wrap font-sans text-xs leading-relaxed text-foreground">
            {routeConfirmText}
          </pre>
        }
        confirmLabel={t("routes.actions.confirmSave")}
        cancelLabel={t("routes.actions.backToEdit")}
        danger={false}
        loading={routeSaving}
      />

      <ConfirmDialog
        open={bindingConfirmOpen}
        onClose={() => setBindingConfirmOpen(false)}
        onConfirm={() => void executeSaveBinding()}
        title={t("routes.dialogs.confirmSaveBinding")}
        description={
          <pre className="whitespace-pre-wrap font-sans text-xs leading-relaxed text-foreground">
            {bindingConfirmText}
          </pre>
        }
        confirmLabel={t("routes.actions.confirmSave")}
        cancelLabel={t("routes.actions.backToEdit")}
        danger={false}
        loading={bindingSaving}
      />

      {/* Delete Confirm Dialogs */}
      <ConfirmDialog
        open={Boolean(deleteRouteTarget)}
        onClose={() => setDeleteRouteTarget(null)}
        onConfirm={() => void handleConfirmDeleteRoute()}
        title={t("common.warning")}
        description={
          deleteRouteTarget
            ? t("routes.messages.deleteRouteConfirm", {
                tenant: routeTenantName(deleteRouteTarget),
                product: routeProductName(deleteRouteTarget),
              })
            : ""
        }
        confirmLabel={t("common.confirm")}
        cancelLabel={t("common.cancel")}
        loading={deleting}
      />

      <ConfirmDialog
        open={Boolean(batchDeleteIds)}
        onClose={() => setBatchDeleteIds(null)}
        onConfirm={() => void handleConfirmBatchDelete()}
        title={t("common.warning")}
        description={t("common.actions.delete")}
        confirmLabel={t("common.confirm")}
        cancelLabel={t("common.cancel")}
        loading={deleting}
      />

      <ConfirmDialog
        open={Boolean(deleteBindingTarget)}
        onClose={() => setDeleteBindingTarget(null)}
        onConfirm={() => void handleConfirmDeleteBinding()}
        title={t("common.warning")}
        description={
          deleteBindingTarget
            ? t("routes.messages.deleteBindingConfirm", {
                name: channelName(deleteBindingTarget.channel_id),
              })
            : ""
        }
        confirmLabel={t("common.confirm")}
        cancelLabel={t("common.cancel")}
        loading={deleting}
      />

      {/* Emergency Switch Modal */}
      <EmergencySwitchModal
        open={switchOpen}
        onClose={() => setSwitchOpen(false)}
        saving={switching}
        channelName={channelName(switchChannelId)}
        reason={switchReason}
        onReasonChange={setSwitchReason}
        onConfirm={handleSubmitEmergencySwitch}
      />

      {/* CSV Import Modal */}
      <CsvImportModal
        open={importOpen}
        onClose={() => setImportOpen(false)}
        title={t("routes.import.title")}
        columns={routeImportColumns}
        formatHint={routeImportFormatHint}
        uniqueKey="key"
        existingData={routeImportExistingData}
        validator={validateRouteImportRow}
        transformer={transformRouteImportRow}
        onImport={handleRouteImport}
        showSuccessMessage={false}
        onSuccess={() => void loadData()}
      />
    </div>
  );
}
