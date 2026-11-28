import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { Plus, RefreshCw, Trash2, Upload } from "lucide-react";
import {
  createProduct,
  createProductReservation,
  deleteProduct,
  fetchProductList,
  releaseProductReservation,
} from "@/api/product";
import { CsvImportModal } from "@/components/common/CsvImportModal";
import { ExportDropdown } from "@/components/common/ExportDropdown";
import {
  Badge,
  Button,
  ConfirmDialog,
  CopyableText,
  DataTablePage,
  ListQueryBar,
  StatusBadge,
  type DataTableColumn,
} from "@/components/ui";
import { productsCatalog } from "@/list-catalog/common-resources";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import type { Product, ProductIdReservation } from "@/types/api/product";
import type { ListFilter } from "@/types/list-query";
import { EMPTY_PLACEHOLDER, formatDateTime } from "@/utils/format";
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
import { CreateProductModal } from "./CreateProductModal";

interface ProductImportRow extends Record<string, unknown> {
  name: string;
  enabled?: string;
}

interface ProductExportRow extends Record<string, unknown> {
  product_id: string;
  name: string;
  enabled: boolean;
  created_at: string;
}

function parseBoolean(value?: string): boolean | undefined {
  const normalized = value?.trim().toLowerCase();
  if (!normalized) return undefined;
  if (["true", "1", "yes", "y", "enabled", "on"].includes(normalized)) {
    return true;
  }
  if (["false", "0", "no", "n", "disabled", "off"].includes(normalized)) {
    return false;
  }
  return undefined;
}

export function ProductListTab() {
  const { t } = useT();
  const navigate = useNavigate();
  const role = useAuthStore((s) => s.info.role);
  const canWrite = isBizRole(role);

  const [loading, setLoading] = useState(false);
  const [rows, setRows] = useState<Product[]>([]);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(10);
  const [total, setTotal] = useState(0);
  const [qInput, setQInput] = useState("");
  const [appliedQ, setAppliedQ] = useState("");
  const [listFilters, setListFilters] = useState<ListFilter[]>([]);
  const [selectedIds, setSelectedIds] = useState<string[]>([]);

  const [createOpen, setCreateOpen] = useState(false);
  const [reserving, setReserving] = useState(false);
  const [activeReservation, setActiveReservation] =
    useState<ProductIdReservation | null>(null);
  const reservationRef = useRef<ProductIdReservation | null>(null);
  const reservationConsumedRef = useRef(false);

  const [importOpen, setImportOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<Product | null>(null);
  const [batchDeleteIds, setBatchDeleteIds] = useState<string[] | null>(null);
  const [deleting, setDeleting] = useState(false);

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const res = await fetchProductList({
        q: appliedQ || undefined,
        filters: encodeFilters(listFilters).filters,
        page,
        page_size: pageSize,
      });
      setRows(res.products ?? []);
      setTotal(res.pagination?.total ?? 0);
      setSelectedIds([]);
    } catch (error) {
      logger.error("Failed to load product list:", error);
    } finally {
      setLoading(false);
    }
  }, [appliedQ, listFilters, page, pageSize]);

  useEffect(() => {
    void loadData();
  }, [loadData]);

  const releaseActiveReservation = useCallback(async () => {
    const reservation = reservationRef.current;
    if (!reservation || reservationConsumedRef.current) return;
    reservationRef.current = null;
    setActiveReservation(null);
    try {
      await releaseProductReservation(reservation.id);
    } catch (error) {
      logger.error("Failed to release product_id reservation:", error);
    }
  }, []);

  useEffect(() => {
    return () => {
      void releaseActiveReservation();
    };
  }, [releaseActiveReservation]);

  const handleOpenCreate = async () => {
    if (reserving) return;
    setReserving(true);
    try {
      const reservation = await createProductReservation();
      reservationConsumedRef.current = false;
      reservationRef.current = reservation;
      setActiveReservation(reservation);
      setCreateOpen(true);
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to reserve product_id:", error);
      }
    } finally {
      setReserving(false);
    }
  };

  const handleCloseCreate = () => {
    setCreateOpen(false);
    void releaseActiveReservation();
  };

  const handleCreateSuccess = async () => {
    reservationConsumedRef.current = true;
    reservationRef.current = null;
    setActiveReservation(null);
    setCreateOpen(false);
    await loadData();
  };

  const handleSearch = useCallback(() => {
    setPage(1);
    setAppliedQ(qInput.trim());
  }, [qInput]);

  const handleFiltersChange = useCallback((next: ListFilter[]) => {
    setPage(1);
    setListFilters(next);
  }, []);

  const handleConfirmSingleDelete = async () => {
    if (!deleteTarget || deleting) return;
    setDeleting(true);
    try {
      await deleteProduct(deleteTarget.product_id);
      showSuccessToast(t("common.feedback.deleteSuccess"));
      setDeleteTarget(null);
      await loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to delete product:", error);
      }
    } finally {
      setDeleting(false);
    }
  };

  const handleConfirmBatchDelete = async () => {
    if (!batchDeleteIds || batchDeleteIds.length === 0 || deleting) return;
    setDeleting(true);
    try {
      for (const productId of batchDeleteIds) {
        await deleteProduct(productId);
      }
      showSuccessToast(t("common.feedback.deleteSuccess"));
      setBatchDeleteIds(null);
      setSelectedIds([]);
      await loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to batch delete products:", error);
      }
    } finally {
      setDeleting(false);
    }
  };

  const productImportColumns = useMemo<ColumnConfig[]>(
    () => [
      { key: "name", label: t("product.fields.name"), required: true },
      { key: "enabled", label: t("product.fields.enabled"), required: false },
    ],
    [t],
  );

  const productImportFormatHint = useMemo(
    () =>
      [
        t("feedback.import.csvFormatHint"),
        t("feedback.import.requiredColumns", {
          columns: productImportColumns
            .filter((c) => c.required)
            .map((c) => c.key)
            .join(", "),
        }),
      ].join("\n"),
    [t, productImportColumns],
  );

  const validateProductImportRow = (row: Record<string, string>) => {
    const trimmedName = row.name?.trim() ?? "";
    if (!trimmedName) {
      return {
        valid: false,
        reason: t("product.createDialog.validation.nameRequired"),
      };
    }
    if (trimmedName.length < 2 || trimmedName.length > 100) {
      return {
        valid: false,
        reason: t("product.createDialog.validation.nameLength"),
      };
    }
    return { valid: true };
  };

  const transformProductImportRow = (
    row: Record<string, string>,
  ): ProductImportRow => ({
    name: row.name.trim(),
    enabled: row.enabled?.trim() || "",
  });

  const handleProductImport = async (items: Record<string, unknown>[]) => {
    await runResourceCsvImport<ProductImportRow>(
      items,
      async (row) => {
        let reservation: ProductIdReservation | null = null;
        try {
          reservation = await createProductReservation();
          await createProduct({
            reservation_id: reservation.id,
            name: row.name,
            enabled: parseBoolean(row.enabled) ?? true,
          });
          reservation = null;
        } finally {
          if (reservation) {
            void releaseProductReservation(reservation.id).catch((err) => {
              logger.error("Failed to release import reservation:", err);
            });
          }
        }
      },
      {
        onError: (error) => {
          logger.error("Failed to import product:", error);
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
        ? rows.filter((r) => selectedIds.includes(r.product_id))
        : rows;
    const csvRows: ProductExportRow[] = targetRows.map((row) => ({
      product_id: row.product_id,
      name: row.name,
      enabled: row.enabled,
      created_at: row.created_at?.trim() ? formatDateTime(row.created_at) : "",
    }));
    exportResourceCsv(
      csvRows,
      [
        { key: "product_id", label: t("product.fields.productId") },
        { key: "name", label: t("product.fields.name") },
        { key: "enabled", label: t("product.fields.enabled") },
        { key: "created_at", label: t("common.fields.createdAt") },
      ],
      `products-${command}-${Date.now().toString(36)}.csv`,
    );
  };

  const columns = useMemo<DataTableColumn<Product>[]>(
    () => [
      {
        id: "product_id",
        header: t("product.columns.productId"),
        width: 170,
        cell: (row) => <CopyableText value={row.product_id} />,
      },
      {
        id: "name",
        header: t("product.columns.name"),
        minWidth: 180,
        cell: (row) => (
          <button
            type="button"
            onClick={() => navigate(`/operations/products/${row.product_id}`)}
            className="text-left font-medium text-foreground hover:text-primary transition-colors cursor-pointer truncate max-w-[280px] block"
            title={row.name}
          >
            {row.name || EMPTY_PLACEHOLDER}
          </button>
        ),
      },
      {
        id: "timeout_secs",
        header: t("product.fields.timeout"),
        width: 110,
        align: "right",
        mono: true,
        cell: (row) =>
          row.timeout_secs != null
            ? `${row.timeout_secs}s`
            : EMPTY_PLACEHOLDER,
      },
      {
        id: "cb_pro_enabled",
        header: t("product.fields.cbProEnabled"),
        width: 112,
        cell: (row) => (
          <Badge variant={row.cb_pro_enabled ? "xblue" : "neutral"}>
            {row.cb_pro_enabled
              ? t("common.status.enabled")
              : t("common.status.disabled")}
          </Badge>
        ),
      },
      {
        id: "enabled",
        header: t("product.columns.enabled"),
        width: 108,
        cell: (row) => (
          <StatusBadge
            status={row.enabled ? "ENABLED" : "DISABLED"}
            label={
              row.enabled
                ? t("common.status.enabled")
                : t("common.status.disabled")
            }
          />
        ),
      },
      {
        id: "created_at",
        header: t("common.fields.createdAt"),
        width: 172,
        mono: true,
        cell: (row) => formatDateTime(row.created_at),
      },
      {
        id: "operations",
        header: t("common.fields.operations"),
        width: canWrite ? 140 : 88,
        align: "right",
        cell: (row) => (
          <div className="inline-flex items-center justify-end gap-1">
            <Button
              type="button"
              variant="link"
              size="xs"
              onClick={() => navigate(`/operations/products/${row.product_id}`)}
            >
              {t("common.actions.details")}
            </Button>
            {canWrite && (
              <Button
                type="button"
                variant="link"
                size="xs"
                className="text-muted-foreground hover:text-danger-fg hover:bg-danger-fg/10"
                onClick={() => setDeleteTarget(row)}
              >
                {t("common.actions.delete")}
              </Button>
            )}
          </div>
        ),
      },
    ],
    [t, canWrite, navigate],
  );

  const hasActiveSearch = Boolean(appliedQ) || listFilters.length > 0;

  return (
    <div className="flex flex-col flex-1 min-h-0 min-w-0">
      <DataTablePage
        columns={columns}
        data={rows}
        getRowId={(row) => row.product_id}
        loading={loading}
        emptyText={
          hasActiveSearch
            ? t("product.empty.filtered")
            : t("product.empty.default")
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
        testId="product-list-table"
        filters={
          <ListQueryBar
            q={qInput}
            onQChange={setQInput}
            filters={listFilters}
            onFiltersChange={handleFiltersChange}
            catalog={productsCatalog}
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
                  onClick={() => setBatchDeleteIds(ids)}
                  data-testid="product-batch-delete-btn"
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
            <ExportDropdown
              items={exportItems}
              disabled={rows.length === 0}
              onCommand={handleExportCommand}
            />
            {canWrite && (
              <Button
                type="button"
                variant="primary"
                size="md"
                loading={reserving}
                onClick={() => void handleOpenCreate()}
                data-testid="product-create-btn"
              >
                <Plus className="h-3.5 w-3.5" />
                {t("product.actions.add")}
              </Button>
            )}
          </>
        }
      />

      <CreateProductModal
        open={createOpen}
        reservation={activeReservation}
        onClose={handleCloseCreate}
        onSuccess={handleCreateSuccess}
      />

      <CsvImportModal
        open={importOpen}
        onClose={() => setImportOpen(false)}
        title={t("common.actions.import")}
        columns={productImportColumns}
        formatHint={productImportFormatHint}
        uniqueKey="name"
        validator={validateProductImportRow}
        transformer={transformProductImportRow}
        onImport={handleProductImport}
        showSuccessMessage={false}
        showErrorMessage={false}
        onSuccess={() => void loadData()}
      />

      <ConfirmDialog
        open={Boolean(deleteTarget)}
        onClose={() => setDeleteTarget(null)}
        onConfirm={() => void handleConfirmSingleDelete()}
        title={t("common.actions.delete")}
        description={
          deleteTarget
            ? t("product.messages.deleteProductConfirm", {
                name: deleteTarget.name,
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
        title={t("common.actions.delete")}
        description={t("product.messages.deleteProductConfirm", {
          name: `${batchDeleteIds?.length ?? 0}`,
        })}
        confirmLabel={t("common.confirm")}
        cancelLabel={t("common.cancel")}
        loading={deleting}
      />
    </div>
  );
}
