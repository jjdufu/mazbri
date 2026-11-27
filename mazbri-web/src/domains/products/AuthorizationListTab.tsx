import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router";
import { Plus, RefreshCw, Trash2, Upload } from "lucide-react";
import { fetchAuthorizations } from "@/api/product";
import {
  createDefaultCBProPolicyConfig,
  grantProduct,
  revokeProduct,
} from "@/api/tenant";
import { CsvImportModal } from "@/components/common/CsvImportModal";
import { ExportDropdown } from "@/components/common/ExportDropdown";
import {
  Button,
  ConfirmDialog,
  CopyableText,
  DataTablePage,
  ListQueryBar,
  StatusBadge,
  type DataTableColumn,
} from "@/components/ui";
import { authorizationsCatalog } from "@/list-catalog/common-resources";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import {
  PRODUCT_ID_PATTERN,
  type AuthorizationDetail,
} from "@/types/api/product";
import type { TenantProduct } from "@/types/api/tenant";
import type { ListFilter } from "@/types/list-query";
import { EMPTY_PLACEHOLDER, formatDateTime, formatMoney } from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { encodeFilters } from "@/utils/list-query/serialize";
import { isBizRole } from "@/utils/ops/roles";
import {
  exportResourceCsv,
  runResourceCsvImport,
  type ColumnConfig,
} from "@/utils/resource-csv";
import { logger } from "@/utils/sys/logger";
import { showSuccessToast } from "@/utils/ui/message-guard";
import { AuthorizationConfigModal } from "./AuthorizationConfigModal";
import { CreateAuthorizationModal } from "./CreateAuthorizationModal";

interface AuthorizationImportRow extends Record<string, unknown> {
  unique_key: string;
  tenant_code: string;
  product_code: string;
  unit_price: string;
  cb_pro_fallback: boolean;
}

interface AuthorizationExportRow extends Record<string, unknown> {
  tenant_code: string;
  tenant_name: string;
  product_code: string;
  product_name: string;
  unit_price: string;
  cb_pro_fallback: boolean;
  enabled: boolean;
  created_at: string;
}

function parseBoolean(value?: string): boolean | undefined {
  const normalized = value?.trim().toLowerCase();
  if (!normalized) return undefined;
  if (["true", "1", "yes", "y", "on", "enabled"].includes(normalized)) {
    return true;
  }
  if (["false", "0", "no", "n", "off", "disabled"].includes(normalized)) {
    return false;
  }
  return undefined;
}

function parseUnitPrice(value?: string): string {
  if (!value?.trim()) return "0.001";
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed > 0 ? value.trim() : "";
}

function buildTenantProductFromAuthorization(
  auth: AuthorizationDetail,
): TenantProduct {
  return {
    product_id: auth.product_code,
    name: auth.product_name,
    description: "",
    cb_pro_enabled: auth.cb_pro_enabled,
    config: {
      unit_price: auth.unit_price,
      currency: auth.currency,
      cb_pro_fallback: auth.cb_pro_fallback,
      cbpro: auth.config?.cbpro ?? createDefaultCBProPolicyConfig(),
    },
    granted_at: auth.created_at,
    updated_at: auth.updated_at,
  };
}

export function AuthorizationListTab() {
  const { t } = useT();
  const navigate = useNavigate();
  const role = useAuthStore((s) => s.info.role);
  const canWrite = isBizRole(role);

  const [loading, setLoading] = useState(false);
  const [rows, setRows] = useState<AuthorizationDetail[]>([]);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(10);
  const [total, setTotal] = useState(0);
  const [qInput, setQInput] = useState("");
  const [appliedQ, setAppliedQ] = useState("");
  const [listFilters, setListFilters] = useState<ListFilter[]>([]);
  const [selectedIds, setSelectedIds] = useState<string[]>([]);

  const [createOpen, setCreateOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [configOpen, setConfigOpen] = useState(false);
  const [selectedAuth, setSelectedAuth] = useState<AuthorizationDetail | null>(
    null,
  );
  const [selectedProduct, setSelectedProduct] = useState<TenantProduct | null>(
    null,
  );

  const [revokeTarget, setRevokeTarget] = useState<AuthorizationDetail | null>(
    null,
  );
  const [batchRevokeIds, setBatchRevokeIds] = useState<string[] | null>(null);
  const [revoking, setRevoking] = useState(false);

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const res = await fetchAuthorizations({
        q: appliedQ || undefined,
        filters: encodeFilters(listFilters).filters,
        page,
        page_size: pageSize,
      });
      setRows(res.authorizations ?? []);
      setTotal(res.pagination?.total ?? 0);
      setSelectedIds([]);
    } catch (error) {
      logger.error("Failed to fetch authorizations:", error);
    } finally {
      setLoading(false);
    }
  }, [appliedQ, listFilters, page, pageSize]);

  useEffect(() => {
    void loadData();
  }, [loadData]);

  const handleSearch = useCallback(() => {
    setPage(1);
    setAppliedQ(qInput.trim());
  }, [qInput]);

  const handleFiltersChange = useCallback((next: ListFilter[]) => {
    setPage(1);
    setListFilters(next);
  }, []);

  const handleConfigure = (row: AuthorizationDetail) => {
    setSelectedAuth(row);
    setSelectedProduct(buildTenantProductFromAuthorization(row));
    setConfigOpen(true);
  };

  const handleConfirmRevoke = async () => {
    if (!revokeTarget || revoking) return;
    setRevoking(true);
    try {
      await revokeProduct(revokeTarget.tenant_code, revokeTarget.product_code);
      showSuccessToast(t("authorization.list.revokeSuccess"));
      setRevokeTarget(null);
      await loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to revoke authorization:", error);
      }
    } finally {
      setRevoking(false);
    }
  };

  const handleConfirmBatchRevoke = async () => {
    if (!batchRevokeIds || batchRevokeIds.length === 0 || revoking) return;
    setRevoking(true);
    try {
      const targets = rows.filter((r) => batchRevokeIds.includes(r.id));
      for (const item of targets) {
        await revokeProduct(item.tenant_code, item.product_code);
      }
      showSuccessToast(t("authorization.list.revokeSuccess"));
      setBatchRevokeIds(null);
      setSelectedIds([]);
      await loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to batch revoke authorizations:", error);
      }
    } finally {
      setRevoking(false);
    }
  };

  const importColumns = useMemo<ColumnConfig[]>(
    () => [
      {
        key: "tenant_code",
        label: t("authorization.list.tenantCode"),
        required: true,
      },
      {
        key: "product_code",
        label: t("authorization.list.productCode"),
        required: true,
      },
      {
        key: "unit_price",
        label: t("authorization.list.unitPrice"),
        required: false,
      },
      {
        key: "cb_pro_fallback",
        label: t("authorization.config.cbProFallback"),
        required: false,
      },
    ],
    [t],
  );

  const importFormatHint = useMemo(
    () =>
      [
        t("feedback.import.csvFormatHint"),
        t("feedback.import.requiredColumns", {
          columns: importColumns
            .filter((c) => c.required)
            .map((c) => c.key)
            .join(", "),
        }),
      ].join("\n"),
    [t, importColumns],
  );

  const validateImportRow = (row: Record<string, string>) => {
    if (!row.tenant_code?.trim()) {
      return { valid: false, reason: t("authorization.grantDialog.selectTenant") };
    }
    const productCode = row.product_code?.trim();
    if (!productCode) {
      return {
        valid: false,
        reason: t("authorization.grantDialog.selectProduct"),
      };
    }
    if (!PRODUCT_ID_PATTERN.test(productCode)) {
      return {
        valid: false,
        reason: t("product.createDialog.validation.productIdPattern"),
      };
    }
    const unitPrice = parseUnitPrice(row.unit_price);
    if (!unitPrice) {
      return { valid: false, reason: t("authorization.list.unitPrice") };
    }
    const fallback = row.cb_pro_fallback?.trim();
    if (fallback && parseBoolean(fallback) === undefined) {
      return {
        valid: false,
        reason: `${t("authorization.config.cbProFallback")}: ${fallback}`,
      };
    }
    return { valid: true };
  };

  const transformImportRow = (
    row: Record<string, string>,
  ): AuthorizationImportRow => ({
    unique_key: `${row.tenant_code.trim()}/${row.product_code.trim()}`,
    tenant_code: row.tenant_code.trim(),
    product_code: row.product_code.trim(),
    unit_price: parseUnitPrice(row.unit_price) || "0.001",
    cb_pro_fallback: parseBoolean(row.cb_pro_fallback) ?? false,
  });

  const handleImport = async (items: Record<string, unknown>[]) => {
    await runResourceCsvImport<AuthorizationImportRow>(
      items,
      async (row) => {
        await grantProduct(
          row.tenant_code,
          row.product_code,
          {
            unit_price: row.unit_price,
            cb_pro_fallback: row.cb_pro_fallback,
          },
          { showSuccessMessage: false },
        );
      },
      {
        onError: (error) => {
          logger.error("Failed to import authorization:", error);
        },
      },
    );
  };

  const exportItems = useMemo(
    () => [
      {
        command: "page",
        label: t("common.actions.exportPage"),
        disabled: rows.length === 0,
      },
      {
        command: "selected",
        label: t("common.actions.exportSelected", {
          count: selectedIds.length,
        }),
        disabled: selectedIds.length === 0,
      },
    ],
    [t, rows.length, selectedIds.length],
  );

  const handleExportCommand = (command: string) => {
    const targetRows =
      command === "selected"
        ? rows.filter((r) => selectedIds.includes(r.id))
        : rows;
    const csvRows: AuthorizationExportRow[] = targetRows.map((row) => ({
      tenant_code: row.tenant_code,
      tenant_name: row.tenant_name,
      product_code: row.product_code,
      product_name: row.product_name,
      unit_price: row.unit_price,
      cb_pro_fallback: row.cb_pro_fallback,
      enabled: row.enabled,
      created_at: row.created_at?.trim() ? formatDateTime(row.created_at) : "",
    }));
    exportResourceCsv(
      csvRows,
      [
        { key: "tenant_code", label: t("authorization.list.tenantCode") },
        { key: "tenant_name", label: t("authorization.list.tenantName") },
        { key: "product_code", label: t("authorization.list.productCode") },
        { key: "product_name", label: t("authorization.list.productName") },
        { key: "unit_price", label: t("authorization.list.unitPrice") },
        {
          key: "cb_pro_fallback",
          label: t("authorization.config.cbProFallback"),
        },
        { key: "enabled", label: t("common.fields.status") },
        { key: "created_at", label: t("authorization.list.createdAt") },
      ],
      `authorizations-${command}-${Date.now().toString(36)}.csv`,
    );
  };

  const columns = useMemo<DataTableColumn<AuthorizationDetail>[]>(() => {
    const cols: DataTableColumn<AuthorizationDetail>[] = [
      {
        id: "tenant_code",
        header: t("authorization.list.tenantCode"),
        width: 136,
        cell: (row) => <CopyableText value={row.tenant_code} />,
      },
      {
        id: "tenant_name",
        header: t("authorization.list.tenantName"),
        minWidth: 160,
        cell: (row) => (
          <button
            type="button"
            onClick={() => navigate(`/operations/tenants/${row.tenant_code}`)}
            className="text-left font-medium text-foreground hover:text-primary transition-colors cursor-pointer truncate max-w-[240px] block"
            title={row.tenant_name}
          >
            {row.tenant_name || EMPTY_PLACEHOLDER}
          </button>
        ),
      },
      {
        id: "product_code",
        header: t("authorization.list.productCode"),
        width: 140,
        cell: (row) => <CopyableText value={row.product_code} />,
      },
      {
        id: "product_name",
        header: t("authorization.list.productName"),
        minWidth: 160,
        cell: (row) => (
          <button
            type="button"
            onClick={() => navigate(`/operations/products/${row.product_code}`)}
            className="text-left font-medium text-foreground hover:text-primary transition-colors cursor-pointer truncate max-w-[240px] block"
            title={row.product_name}
          >
            {row.product_name || EMPTY_PLACEHOLDER}
          </button>
        ),
      },
      {
        id: "unit_price",
        header: t("authorization.list.unitPrice"),
        width: 136,
        align: "right",
        mono: true,
        cell: (row) => (
          <span className="tabular-nums">
            {formatMoney(row.unit_price)} {t("common.currencies.cny")}
          </span>
        ),
      },
      {
        id: "cb_pro_fallback",
        header: t("authorization.config.cbProFallback"),
        width: 108,
        cell: (row) => (
          <StatusBadge
            status={row.cb_pro_fallback ? "WARNING" : "DISABLED"}
            label={
              row.cb_pro_fallback
                ? t("common.status.enabled")
                : t("common.status.disabled")
            }
          />
        ),
      },
      {
        id: "created_at",
        header: t("authorization.list.createdAt"),
        width: 168,
        mono: true,
        cell: (row) => formatDateTime(row.created_at),
      },
    ];
    if (canWrite) {
      cols.push({
        id: "operations",
        header: t("common.fields.operations"),
        width: 136,
        align: "right",
        cell: (row) => (
          <div className="inline-flex items-center justify-end gap-1">
            <Button
              type="button"
              variant="link"
              size="xs"
              onClick={() => handleConfigure(row)}
            >
              {t("authorization.list.actions.modify")}
            </Button>
            <Button
              type="button"
              variant="link"
              size="xs"
              className="text-muted-foreground hover:text-danger-fg hover:bg-danger-fg/10"
              onClick={() => setRevokeTarget(row)}
            >
              {t("authorization.list.actions.revoke")}
            </Button>
          </div>
        ),
      });
    }
    return cols;
  }, [t, canWrite, navigate]);

  const hasActiveSearch = Boolean(appliedQ) || listFilters.length > 0;

  return (
    <div className="flex flex-col flex-1 min-h-0 min-w-0">
      <DataTablePage
        columns={columns}
        data={rows}
        getRowId={(row) => row.id}
        loading={loading}
        emptyText={
          hasActiveSearch
            ? t("common.feedback.noMatchingData")
            : t("common.feedback.noData")
        }
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
        testId="authorization-list-table"
        filters={
          <ListQueryBar
            q={qInput}
            onQChange={setQInput}
            filters={listFilters}
            onFiltersChange={handleFiltersChange}
            catalog={authorizationsCatalog}
            onSearch={handleSearch}
          />
        }
        batchActions={
          canWrite
            ? (ids) => (
                <Button
                  type="button"
                  variant="danger"
                  size="md"
                  onClick={() => setBatchRevokeIds(ids)}
                  data-testid="authorization-batch-revoke-btn"
                >
                  <Trash2 className="h-3.5 w-3.5" />
                  {t("authorization.list.actions.batchRevoke")}
                </Button>
              )
            : undefined
        }
        actions={
          <>
            <Button
              type="button"
              variant="ghost"
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
              <Button
                type="button"
                variant="outline"
                size="md"
                onClick={() => setImportOpen(true)}
              >
                <Upload className="h-3.5 w-3.5" />
                {t("common.actions.import")}
              </Button>
            )}
            {canWrite && (
              <ExportDropdown
                items={exportItems}
                disabled={rows.length === 0}
                onCommand={handleExportCommand}
              />
            )}
            {canWrite && (
              <Button
                type="button"
                variant="primary"
                size="md"
                onClick={() => setCreateOpen(true)}
                data-testid="authorization-create-btn"
              >
                <Plus className="h-3.5 w-3.5" />
                {t("authorization.list.actions.create")}
              </Button>
            )}
          </>
        }
      />

      <CreateAuthorizationModal
        open={createOpen}
        onClose={() => setCreateOpen(false)}
        onSuccess={loadData}
      />

      <AuthorizationConfigModal
        open={configOpen}
        tenantCode={selectedAuth?.tenant_code ?? ""}
        product={selectedProduct}
        onClose={() => setConfigOpen(false)}
        onUpdated={loadData}
      />

      <CsvImportModal
        open={importOpen}
        onClose={() => setImportOpen(false)}
        title={t("common.actions.import")}
        columns={importColumns}
        formatHint={importFormatHint}
        uniqueKey="unique_key"
        validator={validateImportRow}
        transformer={transformImportRow}
        onImport={handleImport}
        showSuccessMessage={false}
        showErrorMessage={false}
        onSuccess={() => void loadData()}
      />

      <ConfirmDialog
        open={Boolean(revokeTarget)}
        onClose={() => setRevokeTarget(null)}
        onConfirm={() => void handleConfirmRevoke()}
        title={t("common.tips")}
        description={t("authorization.list.revokeConfirm")}
        confirmLabel={t("authorization.list.actions.revoke")}
        cancelLabel={t("common.cancel")}
        loading={revoking}
      />

      <ConfirmDialog
        open={Boolean(batchRevokeIds)}
        onClose={() => setBatchRevokeIds(null)}
        onConfirm={() => void handleConfirmBatchRevoke()}
        title={t("common.tips")}
        description={t("authorization.list.revokeConfirm")}
        confirmLabel={t("authorization.list.actions.batchRevoke")}
        cancelLabel={t("common.cancel")}
        loading={revoking}
      />
    </div>
  );
}
