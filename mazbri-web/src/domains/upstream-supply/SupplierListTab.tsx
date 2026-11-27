import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router";
import { Plug, Plus, RefreshCw, Trash2, Upload } from "lucide-react";
import {
  createSupplier,
  deleteSupplier,
  fetchSuppliers,
  updateSupplier,
  type Supplier,
} from "@/api/supplier";
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
import { suppliersCatalog } from "@/list-catalog/common-resources";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import type { ListFilter } from "@/types/list-query";
import { EMPTY_PLACEHOLDER } from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { encodeFilters } from "@/utils/list-query/serialize";
import { isBizRole } from "@/utils/ops/roles";
import {
  exportResourceCsv,
  runResourceCsvImport,
  type ColumnConfig,
} from "@/utils/resource-csv";
import { formatUrlDisplay, stripUrlScheme } from "@/utils/sensitive";
import { logger } from "@/utils/sys/logger";
import { showSuccessToast } from "@/utils/ui/message-guard";
import { resolveUpstreamType } from "./supplier-form";

interface SupplierImportRow extends Record<string, unknown> {
  supplier_code: string;
  name: string;
  upstream_url: string;
  status: string;
}

interface SupplierExportRow extends Record<string, unknown> {
  supplier_code: string;
  name: string;
  upstream_url: string;
  status: string;
}

const ALLOWED_STATUSES = new Set(["active", "inactive", "suspended"]);

export interface SupplierListTabProps {
  refreshToken?: number;
  onOpenAccess: () => void;
}

export function SupplierListTab({
  refreshToken = 0,
  onOpenAccess,
}: SupplierListTabProps) {
  const { t } = useT();
  const navigate = useNavigate();
  const role = useAuthStore((s) => s.info.role);
  const canWrite = isBizRole(role);

  const [loading, setLoading] = useState(false);
  const [rows, setRows] = useState<Supplier[]>([]);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [total, setTotal] = useState(0);
  const [qInput, setQInput] = useState("");
  const [appliedQ, setAppliedQ] = useState("");
  const [listFilters, setListFilters] = useState<ListFilter[]>([]);
  const [selectedIds, setSelectedIds] = useState<string[]>([]);

  const [importOpen, setImportOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<Supplier | null>(null);
  const [batchDeleteIds, setBatchDeleteIds] = useState<string[] | null>(null);
  const [deleting, setDeleting] = useState(false);

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const res = await fetchSuppliers({
        q: appliedQ || undefined,
        filters: encodeFilters(listFilters).filters,
        page,
        page_size: pageSize,
      });
      setRows(res.suppliers ?? []);
      setTotal(res.pagination?.total ?? 0);
      setSelectedIds([]);
    } catch (error) {
      logger.error("Failed to fetch suppliers:", error);
    } finally {
      setLoading(false);
    }
  }, [appliedQ, listFilters, page, pageSize]);

  useEffect(() => {
    void loadData();
  }, [loadData, refreshToken]);

  const supplierStatusLabel = useCallback(
    (status: string) => {
      if (status === "active") return t("common.status.enabled");
      if (status === "suspended") return t("supplier.status.suspended");
      return t("common.status.disabled");
    },
    [t],
  );

  const supplierStatusKey = (status: string) => {
    if (status === "active") return "ACTIVE";
    if (status === "suspended") return "SUSPENDED";
    return "DISABLED";
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
      await deleteSupplier(deleteTarget.id);
      showSuccessToast(t("common.feedback.deleteSuccess"));
      setDeleteTarget(null);
      await loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to delete supplier:", error);
      }
    } finally {
      setDeleting(false);
    }
  };

  const handleConfirmBatchDelete = async () => {
    if (!batchDeleteIds || batchDeleteIds.length === 0 || deleting) return;
    setDeleting(true);
    try {
      for (const id of batchDeleteIds) {
        await deleteSupplier(id);
      }
      showSuccessToast(t("common.feedback.deleteSuccess"));
      setBatchDeleteIds(null);
      setSelectedIds([]);
      await loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to batch delete suppliers:", error);
      }
    } finally {
      setDeleting(false);
    }
  };

  const importColumns = useMemo<ColumnConfig[]>(
    () => [
      { key: "supplier_code", label: t("supplier.fields.code"), required: true },
      { key: "name", label: t("supplier.fields.name"), required: true },
      {
        key: "upstream_url",
        label: t("supplier.fields.accessAddress"),
        required: true,
      },
      { key: "status", label: t("supplier.fields.status"), required: false },
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

  const importExistingData = useMemo(
    () => rows.map((r) => ({ supplier_code: r.supplier_code })),
    [rows],
  );

  const validateImportRow = (row: Record<string, string>) => {
    if (!row.supplier_code?.trim()) {
      return { valid: false, reason: t("supplier.validation.codeRequired") };
    }
    if (!row.name?.trim()) {
      return { valid: false, reason: t("supplier.validation.nameRequired") };
    }
    if (!row.upstream_url?.trim()) {
      return {
        valid: false,
        reason: t("supplier.validation.accessAddressRequired"),
      };
    }
    const rawStatus = row.status?.trim();
    if (rawStatus && !ALLOWED_STATUSES.has(rawStatus.toLowerCase())) {
      return {
        valid: false,
        reason: `${t("supplier.fields.status")}: ${rawStatus}`,
      };
    }
    return { valid: true };
  };

  const transformImportRow = (
    row: Record<string, string>,
  ): SupplierImportRow => {
    const rawStatus = row.status?.trim().toLowerCase();
    return {
      supplier_code: row.supplier_code.trim(),
      name: row.name.trim(),
      upstream_url: stripUrlScheme(row.upstream_url.trim()),
      status:
        rawStatus && ALLOWED_STATUSES.has(rawStatus) ? rawStatus : "active",
    };
  };

  const handleImport = async (items: Record<string, unknown>[]) => {
    await runResourceCsvImport<SupplierImportRow>(items, async (row) => {
      const created = await createSupplier({
        supplier_code: row.supplier_code,
        name: row.name,
        upstream_url: row.upstream_url,
        config: { upstream_type: "microservice" },
      });
      if (row.status !== "active") {
        await updateSupplier(created.id, { status: row.status });
      }
    });
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
    const csvRows: SupplierExportRow[] = targetRows.map((row) => ({
      supplier_code: row.supplier_code,
      name: row.name,
      upstream_url: stripUrlScheme(row.upstream_url),
      status: supplierStatusLabel(row.status),
    }));
    exportResourceCsv(
      csvRows,
      [
        { key: "supplier_code", label: t("supplier.fields.code") },
        { key: "name", label: t("supplier.fields.name") },
        { key: "upstream_url", label: t("supplier.fields.accessAddress") },
        { key: "status", label: t("supplier.fields.status") },
      ],
      `suppliers-${command}-${Date.now().toString(36)}.csv`,
    );
  };

  const columns = useMemo<DataTableColumn<Supplier>[]>(
    () => [
      {
        id: "supplier_code",
        header: t("supplier.fields.code"),
        minWidth: 160,
        cell: (row) => <CopyableText value={row.supplier_code} />,
      },
      {
        id: "name",
        header: t("supplier.fields.name"),
        minWidth: 160,
        cell: (row) => (
          <button
            type="button"
            onClick={() =>
              navigate(`/operations/upstream-supply/suppliers/${row.id}`)
            }
            className="text-left font-medium text-foreground hover:text-primary transition-colors cursor-pointer truncate max-w-[260px] block"
            title={row.name}
          >
            {row.name || EMPTY_PLACEHOLDER}
          </button>
        ),
      },
      {
        id: "upstream_type",
        header: t("supplier.fields.upstreamType"),
        width: 120,
        cell: (row) => {
          const isTransparent = resolveUpstreamType(row) === "transparent";
          return (
            <Badge variant={isTransparent ? "neutral" : "outline"}>
              {isTransparent
                ? t("supplier.upstreamOptions.transparent")
                : t("supplier.upstreamOptions.microservice")}
            </Badge>
          );
        },
      },
      {
        id: "upstream_url",
        header: t("supplier.fields.accessAddress"),
        minWidth: 180,
        cell: (row) => (
          <CopyableText
            value={row.upstream_url}
            displayText={formatUrlDisplay(row.upstream_url)}
            truncate
            maxWidthClass="max-w-[260px]"
          />
        ),
      },
      {
        id: "status",
        header: t("supplier.fields.status"),
        width: 108,
        cell: (row) => (
          <StatusBadge
            status={supplierStatusKey(row.status)}
            label={supplierStatusLabel(row.status)}
          />
        ),
      },
      {
        id: "operations",
        header: t("common.fields.operations"),
        width: canWrite ? 136 : 88,
        align: "right",
        cell: (row) => (
          <div className="inline-flex items-center justify-end gap-1">
            <Button
              type="button"
              variant="link"
              size="xs"
              onClick={() =>
                navigate(`/operations/upstream-supply/suppliers/${row.id}`)
              }
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
    [t, canWrite, navigate, supplierStatusLabel],
  );

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
            ? t("supplier.empty.noMatch")
            : t("supplier.empty.noData")
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
        testId="supplier-list-table"
        filters={
          <ListQueryBar
            q={qInput}
            onQChange={setQInput}
            filters={listFilters}
            onFiltersChange={handleFiltersChange}
            catalog={suppliersCatalog}
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
                  data-testid="supplier-batch-delete-btn"
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
                variant="outline"
                size="md"
                onClick={onOpenAccess}
                data-testid="microservice-access-btn"
              >
                <Plus className="h-3.5 w-3.5" />
                {t("upstreamOnboarding.mode.microservice")}
              </Button>
            )}
            {canWrite && (
              <Button
                type="button"
                variant="primary"
                size="md"
                onClick={() => navigate("/operations/upstream-supply/workbench")}
                data-testid="code-free-access-btn"
              >
                <Plug className="h-3.5 w-3.5" />
                {t("upstreamConnector.codeFreeAccess")}
              </Button>
            )}
          </>
        }
      />

      <CsvImportModal
        open={importOpen}
        onClose={() => setImportOpen(false)}
        title={t("common.actions.import")}
        columns={importColumns}
        formatHint={importFormatHint}
        uniqueKey="supplier_code"
        existingData={importExistingData}
        validator={validateImportRow}
        transformer={transformImportRow}
        onImport={handleImport}
        showSuccessMessage={false}
        showErrorMessage={false}
        onSuccess={() => void loadData()}
      />

      <ConfirmDialog
        open={Boolean(deleteTarget)}
        onClose={() => setDeleteTarget(null)}
        onConfirm={() => void handleConfirmSingleDelete()}
        title={t("common.tips")}
        description={
          deleteTarget
            ? t("supplier.messages.deleteConfirm", { name: deleteTarget.name })
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
        title={t("common.tips")}
        description={t("supplier.messages.deleteConfirm", {
          name: `${batchDeleteIds?.length ?? 0}`,
        })}
        confirmLabel={t("common.confirm")}
        cancelLabel={t("common.cancel")}
        loading={deleting}
      />
    </div>
  );
}
