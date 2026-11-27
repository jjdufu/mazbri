import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router";
import {
  Info,
  Loader2,
  Pencil,
  Plus,
  Power,
  RefreshCw,
  Search,
  Trash2,
} from "lucide-react";
import {
  deleteProduct,
  fetchProductById,
  fetchProductTenants,
  updateProduct,
} from "@/api/product";
import {
  createDefaultCBProPolicyConfig,
  fetchTenantProductConfig,
  revokeProduct,
} from "@/api/tenant";
import {
  Badge,
  Button,
  ConfirmDialog,
  CopyableText,
  DataTablePage,
  EntityDetailCard,
  EntityDetailLayout,
  FormField,
  FormModal,
  Input,
  NumberInput,
  StatusBadge,
  Switch,
  Textarea,
  type DataTableColumn,
  type EntityDetailMetaItem,
  type TabOption,
} from "@/components/ui";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import type {
  Product,
  ProductTenantAccess,
  UpdateProductRequest,
} from "@/types/api/product";
import type { TenantProduct } from "@/types/api/tenant";
import { EMPTY_PLACEHOLDER, formatDateTime } from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { isBizRole } from "@/utils/ops/roles";
import { logger } from "@/utils/sys/logger";
import { showSuccessToast } from "@/utils/ui/message-guard";
import { AuthorizationConfigModal } from "./AuthorizationConfigModal";
import { CreateAuthorizationModal } from "./CreateAuthorizationModal";
import { ProductRoutesSection } from "./routes-management/ProductRoutesSection";

type ProductDetailTab = "routes" | "tenants";

function resolveTab(raw: string | null): ProductDetailTab {
  if (raw === "tenants") return "tenants";
  return "routes";
}

export function ProductDetailPage() {
  const { t } = useT();
  const navigate = useNavigate();
  const { id } = useParams<{ id: string }>();
  const [searchParams, setSearchParams] = useSearchParams();
  const role = useAuthStore((s) => s.info.role);
  const canWrite = isBizRole(role);

  const productCode = (id ?? "").trim();
  const activeTab = resolveTab(searchParams.get("tab"));
  const [loadedTabs, setLoadedTabs] = useState<Set<ProductDetailTab>>(
    () => new Set([activeTab]),
  );

  const [loading, setLoading] = useState(false);
  const [product, setProduct] = useState<Product | null>(null);
  const [statusLoading, setStatusLoading] = useState(false);

  // Edit product modal
  const [editOpen, setEditOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  const [editForm, setEditForm] = useState<Required<UpdateProductRequest>>({
    name: "",
    description: "",
    enabled: true,
    timeout_secs: 30,
    cb_pro_enabled: false,
    cb_failure_threshold: 5,
    cb_success_threshold: 2,
    cb_cooldown_secs: 30,
  });
  const [editErrors, setEditErrors] = useState<Record<string, string>>({});

  // Delete product confirm
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);

  // Authorized tenants tab
  const [tenantLoading, setTenantLoading] = useState(false);
  const [tenants, setTenants] = useState<ProductTenantAccess[]>([]);
  const [tenantPage, setTenantPage] = useState(1);
  const [tenantPageSize, setTenantPageSize] = useState(10);
  const [tenantTotal, setTenantTotal] = useState(0);
  const [tenantKeywordInput, setTenantKeywordInput] = useState("");
  const [tenantAppliedKeyword, setTenantAppliedKeyword] = useState("");
  const [grantOpen, setGrantOpen] = useState(false);
  const [revokeTarget, setRevokeTarget] = useState<ProductTenantAccess | null>(
    null,
  );
  const [revoking, setRevoking] = useState(false);
  const [configOpen, setConfigOpen] = useState(false);
  const [configTenantCode, setConfigTenantCode] = useState("");
  const [configProduct, setConfigProduct] = useState<TenantProduct | null>(
    null,
  );
  const tenantAbortRef = useRef<AbortController | null>(null);

  const syncEditForm = useCallback((data: Product) => {
    setEditForm({
      name: data.name,
      description: data.description ?? "",
      enabled: data.enabled,
      timeout_secs: data.timeout_secs,
      cb_pro_enabled: data.cb_pro_enabled,
      cb_failure_threshold: data.cb_failure_threshold ?? 5,
      cb_success_threshold: data.cb_success_threshold ?? 2,
      cb_cooldown_secs: data.cb_cooldown_secs ?? 30,
    });
  }, []);

  const loadProduct = useCallback(async () => {
    if (!productCode) {
      setProduct(null);
      return;
    }
    setLoading(true);
    try {
      const data = await fetchProductById(productCode);
      setProduct(data);
      syncEditForm(data);
    } catch (error) {
      logger.error("Failed to load product:", error);
    } finally {
      setLoading(false);
    }
  }, [productCode, syncEditForm]);

  const loadAuthorizedTenants = useCallback(async () => {
    if (!productCode) {
      setTenants([]);
      setTenantTotal(0);
      return;
    }
    tenantAbortRef.current?.abort();
    const controller = new AbortController();
    tenantAbortRef.current = controller;
    setTenantLoading(true);
    try {
      const res = await fetchProductTenants(
        productCode,
        {
          page: tenantPage,
          page_size: tenantPageSize,
          q: tenantAppliedKeyword || undefined,
        },
        controller.signal,
      );
      if (controller.signal.aborted) return;
      setTenants(res.tenants ?? []);
      setTenantTotal(res.pagination?.total ?? 0);
    } catch (error) {
      if (controller.signal.aborted) return;
      setTenants([]);
      setTenantTotal(0);
      logger.error("Failed to load authorized tenants:", error);
    } finally {
      if (tenantAbortRef.current === controller) {
        setTenantLoading(false);
        tenantAbortRef.current = null;
      }
    }
  }, [productCode, tenantPage, tenantPageSize, tenantAppliedKeyword]);

  useEffect(() => {
    void loadProduct();
  }, [loadProduct]);

  useEffect(() => {
    setLoadedTabs((prev) => {
      if (prev.has(activeTab)) return prev;
      const next = new Set(prev);
      next.add(activeTab);
      return next;
    });
  }, [activeTab]);

  useEffect(() => {
    if (activeTab === "tenants" || loadedTabs.has("tenants")) {
      void loadAuthorizedTenants();
    }
  }, [activeTab, loadedTabs, loadAuthorizedTenants]);

  useEffect(() => {
    return () => {
      tenantAbortRef.current?.abort();
    };
  }, []);

  const handleTabChange = (next: ProductDetailTab) => {
    const nextParams = new URLSearchParams(searchParams);
    nextParams.set("tab", next);
    setSearchParams(nextParams, { replace: true });
  };

  const handleStatusChange = async (enabled: boolean) => {
    if (!productCode || statusLoading) return;
    setStatusLoading(true);
    try {
      const updated = await updateProduct(productCode, { enabled });
      setProduct(updated);
      syncEditForm(updated);
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to change status:", error);
      }
    } finally {
      setStatusLoading(false);
    }
  };

  const handleOpenEdit = () => {
    if (!product) return;
    syncEditForm(product);
    setEditErrors({});
    setEditOpen(true);
  };

  const validateEditForm = (): boolean => {
    const next: Record<string, string> = {};
    const trimmed = editForm.name.trim();
    if (!trimmed) {
      next.name = t("product.detail.validation.nameRequired");
    } else if (trimmed.length < 2 || trimmed.length > 100) {
      next.name = t("product.detail.validation.nameLength");
    }
    if (
      !Number.isFinite(editForm.timeout_secs) ||
      editForm.timeout_secs < 1 ||
      editForm.timeout_secs > 300
    ) {
      next.timeout_secs = t("product.createDialog.validation.timeoutSecsRange");
    }
    if (editForm.cb_pro_enabled) {
      if (!editForm.cb_failure_threshold || editForm.cb_failure_threshold < 1) {
        next.cb_failure_threshold = t(
          "product.detail.validation.cbFailureThresholdRequired",
        );
      }
      if (!editForm.cb_success_threshold || editForm.cb_success_threshold < 1) {
        next.cb_success_threshold = t(
          "product.detail.validation.cbSuccessThresholdRequired",
        );
      }
      if (!editForm.cb_cooldown_secs || editForm.cb_cooldown_secs < 1) {
        next.cb_cooldown_secs = t(
          "product.detail.validation.cbCooldownSecsRequired",
        );
      }
    }
    setEditErrors(next);
    return Object.keys(next).length === 0;
  };

  const handleSaveEdit = async () => {
    if (!validateEditForm() || saving) return;
    setSaving(true);
    try {
      const updated = await updateProduct(productCode, {
        ...editForm,
        name: editForm.name.trim(),
        description: editForm.description.trim(),
      });
      setProduct(updated);
      setEditOpen(false);
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to update product:", error);
      }
    } finally {
      setSaving(false);
    }
  };

  const handleConfirmDelete = async () => {
    if (!productCode || deleting) return;
    setDeleting(true);
    try {
      await deleteProduct(productCode);
      showSuccessToast(t("common.feedback.deleteSuccess"));
      navigate("/operations/product-authorization?tab=products");
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to delete product:", error);
      }
    } finally {
      setDeleting(false);
    }
  };

  const handleConfirmRevokeTenant = async () => {
    if (!revokeTarget || revoking) return;
    setRevoking(true);
    try {
      await revokeProduct(revokeTarget.tenant_code, productCode);
      showSuccessToast(t("authorization.list.revokeSuccess"));
      setRevokeTarget(null);
      if (tenants.length === 1 && tenantPage > 1) {
        setTenantPage((p) => p - 1);
      } else {
        await loadAuthorizedTenants();
      }
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to revoke tenant product access:", error);
      }
    } finally {
      setRevoking(false);
    }
  };

  const handleOpenTenantAuthConfig = async (row: ProductTenantAccess) => {
    setConfigTenantCode(row.tenant_code);
    try {
      const latest = await fetchTenantProductConfig(
        row.tenant_code,
        productCode,
      );
      setConfigProduct(latest);
      setConfigOpen(true);
    } catch {
      setConfigProduct({
        product_id: productCode,
        name: product?.name ?? productCode,
        cb_pro_enabled: product?.cb_pro_enabled,
        config: {
          unit_price: "0.001",
          currency: "CNY",
          cb_pro_fallback: false,
          cbpro: createDefaultCBProPolicyConfig(),
        },
        granted_at: row.created_at,
      });
      setConfigOpen(true);
    }
  };

  const tenantColumns = useMemo<DataTableColumn<ProductTenantAccess>[]>(
    () => [
      {
        id: "tenant_name",
        header: t("product.detail.tenantColumns.name"),
        minWidth: 180,
        cell: (row) => (
          <button
            type="button"
            onClick={() => navigate(`/operations/tenants/${row.tenant_code}`)}
            className="text-left font-medium text-foreground hover:text-primary transition-colors cursor-pointer truncate max-w-[260px] block"
          >
            {row.tenant_name || row.tenant_code || EMPTY_PLACEHOLDER}
          </button>
        ),
      },
      {
        id: "tenant_id",
        header: t("product.detail.tenantColumns.tenantId"),
        minWidth: 150,
        cell: (row) => (
          <CopyableText
            value={row.tenant_id}
            truncate
            maxWidthClass="max-w-[160px]"
          />
        ),
      },
      {
        id: "tenant_code",
        header: t("product.detail.tenantColumns.tenantCode"),
        width: 140,
        cell: (row) => <CopyableText value={row.tenant_code} />,
      },
      {
        id: "tenant_enabled",
        header: t("common.fields.status"),
        width: 100,
        cell: (row) => (
          <StatusBadge
            status={row.tenant_enabled ? "ENABLED" : "DISABLED"}
            label={
              row.tenant_enabled
                ? t("common.status.enabled")
                : t("common.status.disabled")
            }
          />
        ),
      },
      {
        id: "created_at",
        header: t("product.detail.tenantColumns.grantedAt"),
        width: 168,
        mono: true,
        cell: (row) => formatDateTime(row.created_at),
      },
      {
        id: "operations",
        header: t("common.fields.operations"),
        width: canWrite ? 196 : 96,
        align: "right",
        cell: (row) => (
          <div className="inline-flex items-center justify-end gap-1">
            <Button
              type="button"
              variant="link"
              size="xs"
              onClick={() => navigate(`/operations/tenants/${row.tenant_code}`)}
            >
              {t("common.actions.details")}
            </Button>
            {canWrite && (
              <>
                <Button
                  type="button"
                  variant="link"
                  size="xs"
                  onClick={() => void handleOpenTenantAuthConfig(row)}
                >
                  {t("authorization.list.actions.modify")}
                </Button>
                <Button
                  type="button"
                  variant="link"
                  size="xs"
                  className="text-danger-fg hover:text-danger-fg hover:bg-danger-fg/10"
                  onClick={() => setRevokeTarget(row)}
                >
                  {t("authorization.list.actions.revoke")}
                </Button>
              </>
            )}
          </div>
        ),
      },
    ],
    [t, canWrite, navigate],
  );

  const tabItems: TabOption<ProductDetailTab>[] = [
    {
      key: "routes",
      label: t("product.detail.tabs.routes"),
      testId: "tab-routes",
    },
    {
      key: "tenants",
      label: t("product.detail.tabs.tenants"),
      testId: "tab-tenants",
    },
  ];

  const isEnabled = product?.enabled !== false;

  const productMeta = useMemo<EntityDetailMetaItem[]>(() => {
    const items: EntityDetailMetaItem[] = [
      {
        label: t("product.fields.timeout"),
        value: (
          <span className="font-mono-tech tabular-nums">
            {product?.timeout_secs != null
              ? `${product.timeout_secs} ${t("ops.units.s")}`
              : EMPTY_PLACEHOLDER}
          </span>
        ),
      },
      {
        label: t("product.fields.cbProEnabled"),
        value: product?.cb_pro_enabled ? (
          <span className="font-mono-tech">
            {t("common.status.enabled")} ({product.cb_failure_threshold}/
            {product.cb_success_threshold}/{product.cb_cooldown_secs}
            {t("ops.units.s")})
          </span>
        ) : (
          t("common.status.disabled")
        ),
      },
      {
        label: t("common.fields.createdAt"),
        value: (
          <span className="font-mono-tech">
            {formatDateTime(product?.created_at)}
          </span>
        ),
      },
    ];
    if (product?.description?.trim()) {
      items.push({
        label: t("product.fields.description"),
        value: product.description.trim(),
      });
    }
    return items;
  }, [product, t]);

  return (
    <>
      <EntityDetailLayout
        title={product?.name || productCode}
        code={product?.product_id || productCode}
        status={isEnabled ? "ENABLED" : "DISABLED"}
        statusLabel={
          isEnabled ? t("common.status.enabled") : t("common.status.disabled")
        }
        badges={
          <Badge variant="outline">
            {t("product.detail.billingModePayPerCall")}
          </Badge>
        }
        meta={productMeta}
        onBack={() => navigate("/operations/product-authorization?tab=products")}
        actions={
          canWrite ? (
            <>
              <Button
                type="button"
                variant="outline"
                size="md"
                disabled={!product}
                onClick={handleOpenEdit}
                data-testid="product-detail-edit-btn"
              >
                <Pencil className="h-3.5 w-3.5" />
                {t("common.actions.edit")}
              </Button>
              {product && (
                <Button
                  type="button"
                  variant={product.enabled ? "warning" : "primary"}
                  size="md"
                  loading={statusLoading}
                  onClick={() => void handleStatusChange(!product.enabled)}
                  data-testid="product-detail-toggle-btn"
                >
                  <Power className="h-3.5 w-3.5" />
                  {product.enabled
                    ? t("common.actions.disable")
                    : t("common.actions.enable")}
                </Button>
              )}
              <Button
                type="button"
                variant="danger"
                size="md"
                disabled={!product}
                onClick={() => setDeleteOpen(true)}
                data-testid="product-detail-delete-btn"
              >
                <Trash2 className="h-3.5 w-3.5" />
                {t("common.actions.delete")}
              </Button>
            </>
          ) : undefined
        }
        tabs={tabItems}
        activeTab={activeTab}
        onTabChange={handleTabChange}
        testId="product-detail-page"
      >
        {activeTab === "routes" && loading && !product && (
          <div className="flex items-center justify-center gap-2 py-10 text-xs text-muted-foreground">
            <Loader2 className="h-4 w-4 animate-spin text-primary" />
          </div>
        )}

        {product?.id && loadedTabs.has("routes") && (
          <div className={activeTab === "routes" ? "min-w-0" : "hidden"}>
            <EntityDetailCard
              title={t("routes.embeddedTitle")}
              bodyClassName="p-0"
              testId="product-routes-card"
            >
              <ProductRoutesSection
                embedded
                initialProductId={product.id}
                lockProduct
                borderless
              />
            </EntityDetailCard>
          </div>
        )}

        {loadedTabs.has("tenants") && (
          <div className={activeTab === "tenants" ? "min-w-0" : "hidden"}>
            <EntityDetailCard
              title={t("product.detail.authorizedTenantsTitle")}
              bodyClassName="p-0"
              testId="product-tenants-card"
            >
              <DataTablePage
                columns={tenantColumns}
                data={tenants}
                getRowId={(row) => row.id}
                loading={tenantLoading}
                emptyText={
                  tenantAppliedKeyword
                    ? t("product.detail.noMatchingTenants")
                    : t("product.detail.noAuthorizedTenants")
                }
                page={tenantPage}
                pageSize={tenantPageSize}
                total={tenantTotal}
                pageSizeOptions={[10, 20, 50]}
                onPageChange={setTenantPage}
                onPageSizeChange={(next) => {
                  setTenantPageSize(next);
                  setTenantPage(1);
                }}
                className="[&>div]:border-0 [&>div]:rounded-none [&>div]:shadow-none"
                filters={
                  <div className="flex items-center gap-1.5">
                    <Input
                      value={tenantKeywordInput}
                      onChange={(e) => setTenantKeywordInput(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") {
                          setTenantPage(1);
                          setTenantAppliedKeyword(tenantKeywordInput.trim());
                        }
                      }}
                      placeholder={t("product.detail.searchTenantsPlaceholder")}
                      className="w-60"
                    />
                    <Button
                      type="button"
                      variant="outline"
                      size="md"
                      onClick={() => {
                        setTenantPage(1);
                        setTenantAppliedKeyword(tenantKeywordInput.trim());
                      }}
                    >
                      <Search className="h-3.5 w-3.5" />
                      {t("common.fields.search")}
                    </Button>
                  </div>
                }
                actions={
                  <>
                    <Button
                      type="button"
                      variant="outline"
                      size="md"
                      onClick={() => void loadAuthorizedTenants()}
                    >
                      <RefreshCw className="h-3.5 w-3.5" />
                      {t("common.actions.refresh")}
                    </Button>
                    {canWrite && (
                      <Button
                        type="button"
                        variant="primary"
                        size="md"
                        onClick={() => setGrantOpen(true)}
                        data-testid="product-grant-tenant-btn"
                      >
                        <Plus className="h-3.5 w-3.5" />
                        {t("product.detail.addAuthorization")}
                      </Button>
                    )}
                  </>
                }
              />
            </EntityDetailCard>
          </div>
        )}
      </EntityDetailLayout>

      {/* Edit Product Modal */}
      <FormModal
        open={editOpen}
        onClose={() => setEditOpen(false)}
        onSubmit={handleSaveEdit}
        title={t("product.detail.editTitle")}
        size="md"
        submitLabel={t("common.actions.save")}
        cancelLabel={t("common.cancel")}
        submitting={saving}
        testId="product-edit-modal"
      >
        <div className="flex items-start gap-2 rounded-md border border-primary/30 bg-primary-soft/45 px-3 py-2 text-xs text-primary">
          <Info className="h-4 w-4 shrink-0 mt-0.5" />
          <span>{t("product.detail.lockedAlert")}</span>
        </div>

        <div className="space-y-3">
          <div className="border-b border-border pb-1.5 text-xs font-semibold text-foreground">
            {t("product.detail.basicInfoTitle")}
          </div>

          <FormField
            label={t("product.fields.name")}
            htmlFor="edit-product-name"
            required
            error={editErrors.name}
          >
            <Input
              id="edit-product-name"
              value={editForm.name}
              onChange={(e) =>
                setEditForm((prev) => ({ ...prev, name: e.target.value }))
              }
            />
          </FormField>

          <FormField
            label={t("product.fields.description")}
            htmlFor="edit-product-desc"
          >
            <Textarea
              id="edit-product-desc"
              rows={2}
              value={editForm.description}
              onChange={(e) =>
                setEditForm((prev) => ({
                  ...prev,
                  description: e.target.value,
                }))
              }
              placeholder={t("product.createDialog.placeholders.description")}
            />
          </FormField>
        </div>

        <div className="space-y-3 pt-1">
          <div className="border-b border-border pb-1.5 text-xs font-semibold text-foreground">
            {t("product.createDialog.sections.advanced")}
          </div>

          <FormField
            label={t("product.fields.timeout")}
            required
            error={editErrors.timeout_secs}
          >
            <div className="flex items-center gap-2">
              <NumberInput
                value={editForm.timeout_secs}
                min={1}
                max={300}
                onChange={(val) =>
                  setEditForm((prev) => ({ ...prev, timeout_secs: val }))
                }
              />
              <span className="text-xs text-muted-foreground shrink-0">
                {t("ops.units.s")}
              </span>
            </div>
          </FormField>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <FormField label={t("product.fields.enabled")}>
              <div className="flex items-center h-8">
                <Switch
                  checked={editForm.enabled}
                  onCheckedChange={(checked) =>
                    setEditForm((prev) => ({ ...prev, enabled: checked }))
                  }
                  aria-label={t("product.fields.enabled")}
                />
              </div>
            </FormField>

            <FormField label={t("product.fields.cbProEnabled")}>
              <div className="flex items-center h-8">
                <Switch
                  checked={editForm.cb_pro_enabled}
                  onCheckedChange={(checked) =>
                    setEditForm((prev) => ({
                      ...prev,
                      cb_pro_enabled: checked,
                    }))
                  }
                  aria-label={t("product.fields.cbProEnabled")}
                />
              </div>
            </FormField>
          </div>

          {editForm.cb_pro_enabled && (
            <div className="grid grid-cols-1 sm:grid-cols-3 gap-3 pt-1">
              <FormField
                label={t("product.fields.cbFailureThresholdLabel")}
                required
                error={editErrors.cb_failure_threshold}
              >
                <div className="flex items-center gap-1.5">
                  <NumberInput
                    value={editForm.cb_failure_threshold}
                    min={1}
                    onChange={(val) =>
                      setEditForm((prev) => ({
                        ...prev,
                        cb_failure_threshold: val,
                      }))
                    }
                  />
                  <span className="text-[11px] text-muted-foreground shrink-0">
                    {t("ops.units.times")}
                  </span>
                </div>
              </FormField>

              <FormField
                label={t("product.fields.cbSuccessThresholdLabel")}
                required
                error={editErrors.cb_success_threshold}
              >
                <div className="flex items-center gap-1.5">
                  <NumberInput
                    value={editForm.cb_success_threshold}
                    min={1}
                    onChange={(val) =>
                      setEditForm((prev) => ({
                        ...prev,
                        cb_success_threshold: val,
                      }))
                    }
                  />
                  <span className="text-[11px] text-muted-foreground shrink-0">
                    {t("ops.units.times")}
                  </span>
                </div>
              </FormField>

              <FormField
                label={t("product.fields.cbCooldownLabel")}
                required
                error={editErrors.cb_cooldown_secs}
              >
                <div className="flex items-center gap-1.5">
                  <NumberInput
                    value={editForm.cb_cooldown_secs}
                    min={1}
                    onChange={(val) =>
                      setEditForm((prev) => ({
                        ...prev,
                        cb_cooldown_secs: val,
                      }))
                    }
                  />
                  <span className="text-[11px] text-muted-foreground shrink-0">
                    {t("ops.units.s")}
                  </span>
                </div>
              </FormField>
            </div>
          )}
        </div>
      </FormModal>

      <CreateAuthorizationModal
        open={grantOpen}
        lockedProduct={product}
        onClose={() => setGrantOpen(false)}
        onSuccess={() => {
          setTenantPage(1);
          void loadAuthorizedTenants();
        }}
      />

      <AuthorizationConfigModal
        open={configOpen}
        tenantCode={configTenantCode}
        product={configProduct}
        onClose={() => setConfigOpen(false)}
        onUpdated={loadAuthorizedTenants}
      />

      <ConfirmDialog
        open={deleteOpen}
        onClose={() => setDeleteOpen(false)}
        onConfirm={() => void handleConfirmDelete()}
        title={t("common.tips")}
        description={t("product.messages.deleteProductConfirm", {
          name: product?.name ?? "",
        })}
        confirmLabel={t("common.confirm")}
        cancelLabel={t("common.cancel")}
        loading={deleting}
      />

      <ConfirmDialog
        open={Boolean(revokeTarget)}
        onClose={() => setRevokeTarget(null)}
        onConfirm={() => void handleConfirmRevokeTenant()}
        title={t("common.tips")}
        description={t("authorization.list.revokeConfirm")}
        confirmLabel={t("authorization.list.actions.revoke")}
        cancelLabel={t("common.cancel")}
        loading={revoking}
      />
    </>
  );
}
