import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { ExternalLink, Plus, RefreshCw, Search, X } from "lucide-react";
import { fetchProductList } from "@/api/product";
import {
  createDefaultCBProPolicyConfig,
  fetchTenantProducts,
  grantProduct,
  revokeProduct,
} from "@/api/tenant";
import {
  Button,
  ConfirmDialog,
  DataTablePage,
  FormField,
  FormModal,
  Input,
  Select,
  StatusBadge,
  Switch,
  type DataTableColumn,
  type SelectOptionItem,
} from "@/components/ui";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import type { Product } from "@/types/api/product";
import type { TenantProduct } from "@/types/api/tenant";
import { EMPTY_PLACEHOLDER, formatDateTime } from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { isBizRole } from "@/utils/ops/roles";
import { logger } from "@/utils/sys/logger";
import { isValidTenantCode, normalizeTenantCode } from "@/utils/tenant-code";
import {
  showSuccessToast,
  showWarningToast,
} from "@/utils/ui/message-guard";
import { ProductConfigModal } from "./ProductConfigModal";

export interface ProductsTabProps {
  tenantCode: string;
}

const UNIT_PRICE_RE = /^(?:0|[1-9]\d{0,11})(?:\.\d{1,6})?$/;

export function ProductsTab({ tenantCode }: ProductsTabProps) {
  const { t } = useT();
  const navigate = useNavigate();
  const role = useAuthStore((s) => s.info.role);
  const canWrite = isBizRole(role);
  const resolvedCode = normalizeTenantCode(tenantCode) ?? "";

  const [rows, setRows] = useState<TenantProduct[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [loading, setLoading] = useState(false);
  const [productActions, setProductActions] = useState<{
    grant: boolean;
    revoke: boolean;
  }>({ grant: false, revoke: false });

  const [qInput, setQInput] = useState("");
  const [appliedQ, setAppliedQ] = useState("");

  // Grant modal state
  const [grantOpen, setGrantOpen] = useState(false);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [availableProducts, setAvailableProducts] = useState<Product[]>([]);
  const [grantProductId, setGrantProductId] = useState("");
  const [grantUnitPrice, setGrantUnitPrice] = useState("0.001");
  const [grantCbProFallback, setGrantCbProFallback] = useState(false);
  const [grantErrors, setGrantErrors] = useState<{
    productId?: string;
    unit_price?: string;
  }>({});
  const [granting, setGranting] = useState(false);

  // Config modal state
  const [configTarget, setConfigTarget] = useState<TenantProduct | null>(null);

  // Revoke confirm state
  const [revokeTarget, setRevokeTarget] = useState<TenantProduct | null>(null);
  const [revoking, setRevoking] = useState(false);

  const abortRef = useRef<AbortController | null>(null);

  const loadData = useCallback(async () => {
    if (!isValidTenantCode(resolvedCode)) {
      setRows([]);
      setTotal(0);
      setProductActions({ grant: false, revoke: false });
      return;
    }
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;
    setLoading(true);
    try {
      const res = await fetchTenantProducts(
        resolvedCode,
        {
          page,
          page_size: pageSize,
          q: appliedQ || undefined,
        },
        controller.signal,
      );
      if (controller.signal.aborted) return;
      setRows(res.products ?? []);
      setTotal(res.pagination?.total ?? 0);
      setProductActions(res.actions ?? { grant: false, revoke: false });
    } catch (error) {
      if (!controller.signal.aborted && !isMfaStepUpCancelled(error)) {
        logger.error("Failed to fetch tenant products:", error);
      }
    } finally {
      if (!controller.signal.aborted) {
        setLoading(false);
      }
    }
  }, [resolvedCode, page, pageSize, appliedQ]);

  useEffect(() => {
    void loadData();
    return () => abortRef.current?.abort();
  }, [loadData]);

  const loadAvailableProducts = useCallback(async () => {
    if (!isValidTenantCode(resolvedCode)) return;
    setCatalogLoading(true);
    try {
      const res = await fetchProductList({ page: 1, page_size: 100 });
      const grantedIds = new Set(rows.map((item) => item.product_id));
      setAvailableProducts(
        (res.products ?? []).filter((p) => !grantedIds.has(p.product_id)),
      );
    } catch (error) {
      logger.error("Failed to fetch product catalog:", error);
    } finally {
      setCatalogLoading(false);
    }
  }, [resolvedCode, rows]);

  const selectedGrantProduct = useMemo(
    () =>
      availableProducts.find((item) => item.product_id === grantProductId) ??
      null,
    [availableProducts, grantProductId],
  );

  useEffect(() => {
    if (!selectedGrantProduct?.cb_pro_enabled) {
      setGrantCbProFallback(false);
    }
  }, [selectedGrantProduct]);

  const handleOpenGrant = () => {
    if (!isValidTenantCode(resolvedCode)) {
      showWarningToast(t("tenant.validation.tenantCodeRequired"));
      return;
    }
    setGrantProductId("");
    setGrantUnitPrice("0.001");
    setGrantCbProFallback(false);
    setGrantErrors({});
    setGrantOpen(true);
    void loadAvailableProducts();
  };

  const handleSubmitGrant = async () => {
    if (granting) return;
    const nextErrors: { productId?: string; unit_price?: string } = {};
    if (!grantProductId) {
      nextErrors.productId = t("tenant.messages.selectProductToGrant");
    }
    const trimmedPrice = grantUnitPrice.trim();
    if (!UNIT_PRICE_RE.test(trimmedPrice)) {
      nextErrors.unit_price = t(
        "tenant.products.grantDialog.unitPricePlaceholder",
      );
    }
    setGrantErrors(nextErrors);
    if (Object.keys(nextErrors).length > 0) return;

    setGranting(true);
    try {
      await grantProduct(resolvedCode, grantProductId, {
        unit_price: trimmedPrice,
        cb_pro_fallback: selectedGrantProduct?.cb_pro_enabled
          ? grantCbProFallback
          : false,
        cbpro: createDefaultCBProPolicyConfig(),
      });
      showSuccessToast(t("common.feedback.authorizationGranted"));
      setGrantOpen(false);
      void loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to grant product:", error);
      }
    } finally {
      setGranting(false);
    }
  };

  const handleConfirmRevoke = async () => {
    if (!revokeTarget || revoking) return;
    setRevoking(true);
    try {
      await revokeProduct(resolvedCode, revokeTarget.product_id);
      showSuccessToast(t("common.feedback.authorizationRevoked"));
      setRevokeTarget(null);
      void loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to revoke product:", error);
      }
    } finally {
      setRevoking(false);
    }
  };

  const productSelectOptions = useMemo<SelectOptionItem[]>(
    () =>
      availableProducts.map((p) => ({
        value: p.product_id,
        text: `${p.name} (${p.product_id})`,
        label: `${p.name} (${p.product_id})`,
      })),
    [availableProducts],
  );

  const columns = useMemo<DataTableColumn<TenantProduct>[]>(
    () => [
      {
        id: "product_id",
        header: t("tenant.products.columns.productId"),
        width: 160,
        mono: true,
        cell: (row) => row.product_id,
      },
      {
        id: "name",
        header: t("tenant.products.columns.name"),
        minWidth: 140,
        cell: (row) => (
          <span className="font-medium text-foreground">{row.name}</span>
        ),
      },
      {
        id: "description",
        header: t("tenant.products.columns.description"),
        minWidth: 140,
        cell: (row) => (
          <span className="text-muted-foreground">
            {row.description || EMPTY_PLACEHOLDER}
          </span>
        ),
      },
      {
        id: "granted_at",
        header: t("tenant.products.columns.grantedAt"),
        width: 168,
        mono: true,
        cell: (row) => formatDateTime(row.granted_at),
      },
      {
        id: "unit_price",
        header: t("tenant.products.columns.unitPrice"),
        width: 132,
        align: "right",
        mono: true,
        cell: (row) =>
          `${row.config.unit_price} ${t("common.currencies.cny")}`,
      },
      {
        id: "cb_pro_fallback",
        header: t("tenant.products.columns.cbProFallback"),
        width: 112,
        cell: (row) =>
          row.cb_pro_enabled ? (
            <StatusBadge
              status={row.config.cb_pro_fallback ? "WARNING" : "DISABLED"}
              label={
                row.config.cb_pro_fallback
                  ? t("common.status.enabled")
                  : t("common.status.disabled")
              }
            />
          ) : (
            <span className="text-muted-foreground">{EMPTY_PLACEHOLDER}</span>
          ),
      },
      ...(canWrite
        ? [
            {
              id: "operations",
              header: t("common.fields.operations"),
              width: 140,
              align: "right" as const,
              cell: (row: TenantProduct) => (
                <div className="inline-flex items-center justify-end gap-1">
                  <Button
                    type="button"
                    variant="link"
                    size="xs"
                    onClick={() => setConfigTarget(row)}
                  >
                    {t("common.actions.modify")}
                  </Button>
                  {productActions.revoke && (
                    <Button
                      type="button"
                      variant="link"
                      size="xs"
                      className="text-danger-fg hover:text-danger-fg"
                      onClick={() => setRevokeTarget(row)}
                    >
                      {t("common.actions.delete")}
                    </Button>
                  )}
                </div>
              ),
            },
          ]
        : []),
    ],
    [canWrite, productActions.revoke, t],
  );

  return (
    <div className="space-y-4 min-w-0">
      <DataTablePage<TenantProduct>
        testId="tenant-products-tab"
        columns={columns}
        data={rows}
        getRowId={(row) => row.product_id}
        loading={loading}
        emptyText={t("common.feedback.noData")}
        page={page}
        pageSize={pageSize}
        total={total}
        pageSizeOptions={[10, 20, 50]}
        onPageChange={setPage}
        onPageSizeChange={(nextSize) => {
          setPageSize(nextSize);
          setPage(1);
        }}
        filters={
          <div className="relative flex w-60 max-w-full items-center">
            <Search className="pointer-events-none absolute left-2.5 h-3.5 w-3.5 text-muted-foreground" />
            <Input
              value={qInput}
              onChange={(e) => setQInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  setPage(1);
                  setAppliedQ(qInput.trim());
                }
              }}
              placeholder={t("common.search")}
              className="pl-8 pr-7"
            />
            {qInput && (
              <button
                type="button"
                onClick={() => {
                  setQInput("");
                  setPage(1);
                  setAppliedQ("");
                }}
                aria-label={t("common.reset")}
                className="absolute right-1.5 inline-flex h-5 w-5 items-center justify-center rounded-md text-muted-foreground hover:bg-hover-bg hover:text-foreground cursor-pointer"
              >
                <X className="h-3 w-3" />
              </button>
            )}
          </div>
        }
        actions={
          <>
            <Button
              type="button"
              variant="outline"
              size="md"
              onClick={() => void loadData()}
              disabled={loading}
            >
              <RefreshCw
                className={loading ? "h-3.5 w-3.5 animate-spin" : "h-3.5 w-3.5"}
              />
              {t("common.actions.refresh")}
            </Button>
            {canWrite && (
              <>
                <Button
                  type="button"
                  variant="outline"
                  size="md"
                  onClick={() =>
                    navigate(
                      "/operations/product-authorization?tab=authorizations",
                    )
                  }
                >
                  <ExternalLink className="h-3.5 w-3.5" />
                  {t("tenant.products.manageGlobalAuth")}
                </Button>
                {productActions.grant && (
                  <Button
                    type="button"
                    variant="primary"
                    size="md"
                    onClick={handleOpenGrant}
                    data-testid="tenant-grant-product-btn"
                  >
                    <Plus className="h-3.5 w-3.5" />
                    {t("tenant.products.grantProduct")}
                  </Button>
                )}
              </>
            )}
          </>
        }
      />

      <FormModal
        open={grantOpen}
        onClose={() => setGrantOpen(false)}
        onSubmit={handleSubmitGrant}
        title={t("tenant.products.grantDialog.title")}
        size="md"
        submitLabel={t("tenant.products.grantDialog.confirm")}
        submitting={granting}
        testId="tenant-grant-product-modal"
      >
        <FormField
          label={t("tenant.products.grantDialog.product")}
          required
          error={grantErrors.productId}
        >
          <Select
            value={grantProductId}
            options={productSelectOptions}
            searchable
            disabled={catalogLoading}
            placeholder={t("tenant.products.grantDialog.productPlaceholder")}
            onChange={(e) => {
              setGrantProductId(e.target.value);
              if (grantErrors.productId) {
                setGrantErrors((prev) => ({ ...prev, productId: undefined }));
              }
            }}
            className="w-full"
          />
        </FormField>

        <FormField
          label={t("tenant.products.grantDialog.unitPrice")}
          htmlFor="grant-product-unit-price"
          required
          error={grantErrors.unit_price}
        >
          <div className="flex items-center gap-2">
            <Input
              id="grant-product-unit-price"
              value={grantUnitPrice}
              onChange={(e) => {
                setGrantUnitPrice(e.target.value);
                if (grantErrors.unit_price) {
                  setGrantErrors((prev) => ({
                    ...prev,
                    unit_price: undefined,
                  }));
                }
              }}
              placeholder={t(
                "tenant.products.grantDialog.unitPricePlaceholder",
              )}
              className="font-mono-tech"
            />
            <span className="shrink-0 whitespace-nowrap text-xs text-muted-foreground">
              {t("common.currencies.cny")}
            </span>
          </div>
        </FormField>

        <div className="flex items-center justify-between gap-4 rounded-md border border-border-subtle px-3 py-2.5">
          <span className="text-xs font-medium text-foreground">
            {t("tenant.products.grantDialog.cbProFallback")}
          </span>
          <Switch
            checked={grantCbProFallback}
            disabled={!selectedGrantProduct?.cb_pro_enabled}
            onCheckedChange={setGrantCbProFallback}
            aria-label={t("tenant.products.grantDialog.cbProFallback")}
          />
        </div>
      </FormModal>

      <ProductConfigModal
        open={Boolean(configTarget)}
        tenantCode={resolvedCode}
        product={configTarget}
        onClose={() => setConfigTarget(null)}
        onUpdated={() => void loadData()}
      />

      <ConfirmDialog
        open={Boolean(revokeTarget)}
        onClose={() => setRevokeTarget(null)}
        onConfirm={() => void handleConfirmRevoke()}
        loading={revoking}
        danger
        title={t("common.tips")}
        confirmLabel={t("common.confirm")}
        description={
          revokeTarget
            ? t("tenant.messages.revokeProductConfirm", {
                name: revokeTarget.name,
              })
            : ""
        }
      />
    </div>
  );
}
