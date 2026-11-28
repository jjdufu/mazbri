import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { Plus, RefreshCw, Trash2, Upload } from "lucide-react";
import {
  createTenant,
  deleteTenant,
  fetchTenantList,
  setTenantEnabled,
} from "@/api/tenant";
import { CsvImportModal } from "@/components/common/CsvImportModal";
import {
  ExportDropdown,
  type ExportDropdownItem,
} from "@/components/common/ExportDropdown";
import {
  Button,
  ConfirmDialog,
  CopyableText,
  DataTablePage,
  StatusBadge,
  type DataTableColumn,
} from "@/components/ui";
import { StructuredFilterBar } from "@/domains/ops/shared/StructuredFilterBar";
import { tenantsCatalog } from "@/list-catalog/tenants";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import type { Tenant } from "@/types/api/tenant";
import type { ListFilter } from "@/types/list-query";
import { getStatusLabel } from "@/utils/display-labels";
import { EMPTY_PLACEHOLDER, formatDateTime } from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { isBizRole } from "@/utils/ops/roles";
import {
  exportResourceCsv,
  runResourceCsvImport,
  type ColumnConfig,
} from "@/utils/resource-csv";
import { logger } from "@/utils/sys/logger";
import { showSuccessToast } from "@/utils/ui/message-guard";
import { TenantCreateModal } from "./TenantCreateModal";

interface TenantImportRow extends Record<string, unknown> {
  tenant_code: string;
  tenant_id: string;
  name: string;
  contact_person: string;
  contact_phone: string;
  contact_email: string;
}

interface TenantExportRow extends Record<string, unknown> {
  tenant_code: string;
  tenant_id: string;
  name: string;
  contact_person: string;
  contact_phone: string;
  contact_email: string;
  status: string;
  created_at: string;
}

export function TenantListPage() {
  const { t } = useT();
  const navigate = useNavigate();
  const role = useAuthStore((s) => s.info.role);
  const canWrite = isBizRole(role);

  const [rows, setRows] = useState<Tenant[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [loading, setLoading] = useState(false);

  const [qInput, setQInput] = useState("");
  const [appliedQ, setAppliedQ] = useState("");
  const [listFilters, setListFilters] = useState<ListFilter[]>([]);
  const [selectedCodes, setSelectedCodes] = useState<string[]>([]);

  const [createOpen, setCreateOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);

  const [toggleTarget, setToggleTarget] = useState<{
    row: Tenant;
    enable: boolean;
  } | null>(null);
  const [toggling, setToggling] = useState(false);

  const [deleteTarget, setDeleteTarget] = useState<Tenant | null>(null);
  const [batchDeleteCodes, setBatchDeleteCodes] = useState<string[] | null>(
    null,
  );
  const [deleting, setDeleting] = useState(false);

  const abortRef = useRef<AbortController | null>(null);

  const loadData = useCallback(async () => {
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;
    setLoading(true);
    try {
      const res = await fetchTenantList(
        {
          page,
          page_size: pageSize,
          q: appliedQ || undefined,
          filters:
            listFilters.length > 0 ? JSON.stringify(listFilters) : undefined,
        },
        controller.signal,
      );
      if (controller.signal.aborted) return;
      setRows(res.tenants ?? []);
      setTotal(res.pagination?.total ?? 0);
      setSelectedCodes([]);
    } catch (error) {
      if (!controller.signal.aborted && !isMfaStepUpCancelled(error)) {
        logger.error("Failed to fetch tenant list:", error);
      }
    } finally {
      if (!controller.signal.aborted) {
        setLoading(false);
      }
    }
  }, [page, pageSize, appliedQ, listFilters]);

  useEffect(() => {
    void loadData();
    return () => abortRef.current?.abort();
  }, [loadData]);

  const handleSearch = useCallback(() => {
    setPage(1);
    setAppliedQ(qInput.trim());
  }, [qInput]);

  const handleFiltersChange = useCallback((next: ListFilter[]) => {
    setPage(1);
    setListFilters(next);
  }, []);

  const selectedRows = useMemo(
    () => rows.filter((row) => selectedCodes.includes(row.tenant_code)),
    [rows, selectedCodes],
  );

  const exportItems = useMemo<ExportDropdownItem[]>(
    () => [
      {
        command: "page",
        label: t("common.actions.exportPage"),
        disabled: rows.length === 0,
      },
      {
        command: "selected",
        label: t("common.actions.exportSelected", {
          count: selectedRows.length,
        }),
        disabled: selectedRows.length === 0,
      },
    ],
    [rows.length, selectedRows.length, t],
  );

  const tenantExportColumns = useMemo<
    { key: keyof TenantExportRow; label: string }[]
  >(
    () => [
      { key: "tenant_code", label: t("tenant.fields.code") },
      { key: "tenant_id", label: t("tenant.fields.accountId") },
      { key: "name", label: t("common.fields.name") },
      { key: "contact_person", label: t("tenant.fields.contactPerson") },
      { key: "contact_phone", label: t("tenant.fields.contactPhone") },
      { key: "contact_email", label: t("tenant.fields.contactEmail") },
      { key: "status", label: t("common.fields.status") },
      { key: "created_at", label: t("common.fields.createdAt") },
    ],
    [t],
  );

  const tenantImportColumns = useMemo<ColumnConfig[]>(
    () => [
      { key: "tenant_code", label: t("tenant.fields.code"), required: true },
      {
        key: "tenant_id",
        label: t("tenant.fields.accountId"),
        required: true,
      },
      { key: "name", label: t("common.fields.name"), required: true },
      {
        key: "contact_person",
        label: t("tenant.fields.contactPerson"),
        required: true,
      },
      {
        key: "contact_phone",
        label: t("tenant.fields.contactPhone"),
        required: true,
      },
      {
        key: "contact_email",
        label: t("tenant.fields.contactEmail"),
        required: true,
      },
    ],
    [t],
  );

  const tenantImportFormatHint = useMemo(
    () =>
      [
        t("feedback.import.csvFormatHint"),
        t("feedback.import.requiredColumns", {
          columns: tenantImportColumns
            .filter((col) => col.required)
            .map((col) => col.key)
            .join(", "),
        }),
      ].join("\n"),
    [t, tenantImportColumns],
  );

  const validateTenantImportRow = useCallback(
    (row: Record<string, string>) => {
      if (!row.tenant_code?.trim()) {
        return {
          valid: false,
          reason: t("tenant.createDialog.validation.codeRequired"),
        };
      }
      if (!row.tenant_id?.trim()) {
        return {
          valid: false,
          reason: t("tenant.createDialog.validation.accountIdRequired"),
        };
      }
      if (!row.name?.trim()) {
        return {
          valid: false,
          reason: t("tenant.createDialog.validation.nameRequired"),
        };
      }
      if (!row.contact_person?.trim()) {
        return {
          valid: false,
          reason: t("tenant.createDialog.validation.contactPersonRequired"),
        };
      }
      if (!row.contact_phone?.trim()) {
        return {
          valid: false,
          reason: t("tenant.createDialog.validation.contactPhoneRequired"),
        };
      }
      if (!row.contact_email?.trim()) {
        return {
          valid: false,
          reason: t("tenant.createDialog.validation.contactEmailRequired"),
        };
      }
      return { valid: true };
    },
    [t],
  );

  const transformTenantImportRow = useCallback(
    (row: Record<string, string>): TenantImportRow => ({
      tenant_code: row.tenant_code.trim().toUpperCase(),
      tenant_id: row.tenant_id.trim().toLowerCase(),
      name: row.name.trim(),
      contact_person: row.contact_person.trim(),
      contact_phone: row.contact_phone.trim(),
      contact_email: row.contact_email.trim(),
    }),
    [],
  );

  const handleTenantImport = useCallback(
    async (items: Record<string, unknown>[]) => {
      await runResourceCsvImport<TenantImportRow>(
        items,
        async (row) => {
          await createTenant(
            {
              tenant_code: row.tenant_code,
              tenant_id: row.tenant_id,
              name: row.name,
              contact_person: row.contact_person,
              contact_phone: row.contact_phone,
              contact_email: row.contact_email,
            },
            undefined,
            { showSuccessMessage: false },
          );
        },
        {
          onError: (error) => {
            if (!isMfaStepUpCancelled(error)) {
              logger.error("Failed to import tenant:", error);
            }
          },
        },
      );
    },
    [],
  );

  const handleExportCommand = useCallback(
    (command: string) => {
      const targetRows = command === "selected" ? selectedRows : rows;
      const exportRows: TenantExportRow[] = targetRows.map((row) => ({
        tenant_code: row.tenant_code,
        tenant_id: row.tenant_id,
        name: row.name,
        contact_person: row.contact_person || "",
        contact_phone: row.contact_phone || "",
        contact_email: row.contact_email || "",
        status: getStatusLabel(row.enabled ? "enabled" : "disabled"),
        created_at: formatDateTime(row.created_at),
      }));
      exportResourceCsv(
        exportRows,
        tenantExportColumns,
        `tenants-${command}-${Date.now().toString(36)}.csv`,
      );
    },
    [rows, selectedRows, tenantExportColumns],
  );

  const handleConfirmToggleStatus = async () => {
    if (!toggleTarget || toggling) return;
    setToggling(true);
    try {
      await setTenantEnabled(toggleTarget.row.tenant_code, {
        enabled: toggleTarget.enable,
      });
      showSuccessToast(
        toggleTarget.enable
          ? t("common.feedback.enabled")
          : t("common.feedback.disabled"),
      );
      setToggleTarget(null);
      void loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to toggle tenant status:", error);
      }
    } finally {
      setToggling(false);
    }
  };

  const handleConfirmSingleDelete = async () => {
    if (!deleteTarget || deleting) return;
    setDeleting(true);
    try {
      await deleteTenant(deleteTarget.tenant_code);
      showSuccessToast(t("common.feedback.deleteSuccess"));
      setDeleteTarget(null);
      void loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to delete tenant:", error);
      }
    } finally {
      setDeleting(false);
    }
  };

  const handleConfirmBatchDelete = async () => {
    if (!batchDeleteCodes || batchDeleteCodes.length === 0 || deleting) return;
    setDeleting(true);
    let deletedCount = 0;
    try {
      for (const code of batchDeleteCodes) {
        try {
          await deleteTenant(code);
          deletedCount += 1;
        } catch (error) {
          if (isMfaStepUpCancelled(error)) {
            break;
          }
          logger.error(`Failed to delete tenant ${code}:`, error);
        }
      }
      if (deletedCount > 0) {
        showSuccessToast(t("common.feedback.deleteSuccess"));
        setSelectedCodes([]);
        void loadData();
      }
      setBatchDeleteCodes(null);
    } finally {
      setDeleting(false);
    }
  };

  const columns = useMemo<DataTableColumn<Tenant>[]>(
    () => [
      {
        id: "tenant_code",
        header: t("tenant.fields.code"),
        width: 112,
        className: "px-2",
        headerClassName: "px-2",
        cell: (row) => <CopyableText value={row.tenant_code} mono />,
      },
      {
        id: "tenant_id",
        header: t("tenant.fields.accountId"),
        minWidth: 116,
        className: "px-2",
        headerClassName: "px-2",
        cell: (row) => (
          <CopyableText
            value={row.tenant_id}
            mono
            truncate
            maxWidthClass="max-w-[116px]"
          />
        ),
      },
      {
        id: "name",
        header: t("common.fields.name"),
        minWidth: 116,
        className: "px-2",
        headerClassName: "px-2",
        cell: (row) => (
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              navigate(`/operations/tenants/${row.tenant_code}`);
            }}
            className="block max-w-[132px] truncate text-left font-medium text-primary hover:underline cursor-pointer"
            title={row.name}
          >
            {row.name || EMPTY_PLACEHOLDER}
          </button>
        ),
      },
      {
        id: "contact_person",
        header: t("tenant.fields.contactPerson"),
        minWidth: 88,
        className: "px-2",
        headerClassName: "px-2",
        cell: (row) => (
          <span
            className="block max-w-[100px] truncate text-foreground"
            title={row.contact_person || undefined}
          >
            {row.contact_person || EMPTY_PLACEHOLDER}
          </span>
        ),
      },
      {
        id: "contact_phone",
        header: t("tenant.fields.contactPhone"),
        width: 108,
        mono: true,
        className: "px-2",
        headerClassName: "px-2",
        cell: (row) => (
          <span className="whitespace-nowrap text-[11px] tabular-nums">
            {row.contact_phone || EMPTY_PLACEHOLDER}
          </span>
        ),
      },
      {
        id: "contact_email",
        header: t("tenant.fields.contactEmail"),
        minWidth: 120,
        className: "px-2",
        headerClassName: "px-2",
        cell: (row) => (
          <span
            className="block max-w-[136px] truncate text-muted-foreground"
            title={row.contact_email || undefined}
          >
            {row.contact_email || EMPTY_PLACEHOLDER}
          </span>
        ),
      },
      {
        id: "status",
        header: t("common.fields.status"),
        width: 72,
        align: "center",
        className: "px-2",
        headerClassName: "px-2",
        cell: (row) => (
          <StatusBadge
            status={row.enabled ? "ENABLED" : "DISABLED"}
            label={getStatusLabel(row.enabled ? "enabled" : "disabled")}
          />
        ),
      },
      {
        id: "created_at",
        header: t("common.fields.createdAt"),
        minWidth: 132,
        mono: true,
        className: "px-2",
        headerClassName: "px-2",
        cell: (row) => (
          <span className="whitespace-nowrap text-[11px] tabular-nums">
            {formatDateTime(row.created_at)}
          </span>
        ),
      },
      {
        id: "operations",
        header: t("common.fields.operations"),
        width: canWrite ? 132 : 72,
        align: "right",
        className: "px-2 pr-3",
        headerClassName: "px-2 pr-3",
        cell: (row) => (
          <div
            className="inline-flex items-center justify-end gap-1"
            onClick={(e) => e.stopPropagation()}
          >
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
                  className={
                    row.enabled
                      ? "text-muted-foreground hover:bg-warning-bg hover:text-warning-fg"
                      : "text-muted-foreground hover:bg-success-fg/10 hover:text-success-fg"
                  }
                  onClick={() =>
                    setToggleTarget({ row, enable: !row.enabled })
                  }
                >
                  {row.enabled
                    ? t("tenant.actions.disable")
                    : t("tenant.actions.enable")}
                </Button>
                <Button
                  type="button"
                  variant="link"
                  size="xs"
                  className="text-muted-foreground hover:bg-danger-fg/10 hover:text-danger-fg"
                  onClick={() => setDeleteTarget(row)}
                >
                  {t("common.actions.delete")}
                </Button>
              </>
            )}
          </div>
        ),
      },
    ],
    [canWrite, navigate, t],
  );

  const hasActiveSearch = Boolean(appliedQ) || listFilters.length > 0;

  return (
    <div className="flex flex-col flex-1 min-h-0 gap-4 min-w-0">
      <DataTablePage<Tenant>
        testId="tenant-list-page"
        columns={columns}
        data={rows}
        getRowId={(row) => row.tenant_code}
        loading={loading}
        emptyText={
          hasActiveSearch
            ? t("common.feedback.noMatchingData")
            : t("common.feedback.noData")
        }
        selectable={canWrite}
        selectedKeys={selectedCodes}
        onSelectionChange={setSelectedCodes}
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
          <StructuredFilterBar
            q={qInput}
            onQChange={setQInput}
            filters={listFilters}
            onFiltersChange={handleFiltersChange}
            catalog={tenantsCatalog}
            onSearch={handleSearch}
          />
        }
        batchActions={
          canWrite
            ? (ids) => (
                <Button
                  type="button"
                  variant="danger"
                  size="sm"
                  onClick={() => setBatchDeleteCodes(ids)}
                  data-testid="tenant-batch-delete-btn"
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
              <>
                <Button
                  type="button"
                  variant="outline"
                  size="md"
                  onClick={() => setImportOpen(true)}
                  data-testid="tenant-import-btn"
                >
                  <Upload className="h-3.5 w-3.5" />
                  {t("common.actions.import")}
                </Button>
                <ExportDropdown
                  items={exportItems}
                  disabled={rows.length === 0}
                  onCommand={handleExportCommand}
                  testId="tenant-export-btn"
                />
                <Button
                  type="button"
                  variant="primary"
                  size="md"
                  onClick={() => setCreateOpen(true)}
                  data-testid="tenant-create-btn"
                >
                  <Plus className="h-3.5 w-3.5" />
                  {t("tenant.actions.addTenant")}
                </Button>
              </>
            )}
          </>
        }
      />

      <TenantCreateModal
        open={createOpen}
        onClose={() => setCreateOpen(false)}
        onSuccess={() => void loadData()}
      />

      <CsvImportModal
        open={importOpen}
        onClose={() => setImportOpen(false)}
        title={t("common.actions.import")}
        columns={tenantImportColumns}
        formatHint={tenantImportFormatHint}
        uniqueKey="tenant_code"
        validator={validateTenantImportRow}
        transformer={transformTenantImportRow}
        onImport={handleTenantImport}
        showSuccessMessage={false}
        showErrorMessage={false}
        onSuccess={() => void loadData()}
        testId="tenant-import-modal"
      />

      <ConfirmDialog
        open={Boolean(toggleTarget)}
        onClose={() => setToggleTarget(null)}
        onConfirm={() => void handleConfirmToggleStatus()}
        loading={toggling}
        danger={toggleTarget ? !toggleTarget.enable : false}
        title={toggleTarget?.enable ? t("common.actions.enable") : t("common.actions.disable")}
        confirmLabel={t("common.confirm")}
        description={
          toggleTarget
            ? toggleTarget.enable
              ? t("tenant.messages.enableTenantConfirm", {
                  name: toggleTarget.row.name,
                })
              : t("tenant.messages.disableTenantConfirm", {
                  name: toggleTarget.row.name,
                })
            : ""
        }
      />

      <ConfirmDialog
        open={Boolean(deleteTarget)}
        onClose={() => setDeleteTarget(null)}
        onConfirm={() => void handleConfirmSingleDelete()}
        loading={deleting}
        danger
        title={t("common.actions.delete")}
        confirmLabel={t("common.actions.delete")}
        description={
          deleteTarget
            ? t("tenant.messages.deleteTenantConfirm", {
                name: deleteTarget.name,
              })
            : ""
        }
      />

      <ConfirmDialog
        open={Boolean(batchDeleteCodes && batchDeleteCodes.length > 0)}
        onClose={() => setBatchDeleteCodes(null)}
        onConfirm={() => void handleConfirmBatchDelete()}
        loading={deleting}
        danger
        title={t("common.actions.delete")}
        confirmLabel={t("common.actions.delete")}
        description={
          batchDeleteCodes
            ? t("feedback.batch.confirmSelectedAction", {
                action: t("common.actions.delete"),
                count: batchDeleteCodes.length,
              })
            : ""
        }
      />
    </div>
  );
}
