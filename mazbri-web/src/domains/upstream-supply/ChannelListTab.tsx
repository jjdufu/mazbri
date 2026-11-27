import { useCallback, useEffect, useMemo, useState } from "react";
import { Plus, RefreshCw, Search, Trash2, Upload } from "lucide-react";
import {
  createChannel,
  deleteChannel,
  fetchChannels,
  updateChannel,
  type Channel,
} from "@/api/route-management";
import { fetchSuppliers, type Supplier } from "@/api/supplier";
import { CsvImportModal } from "@/components/common/CsvImportModal";
import { ExportDropdown } from "@/components/common/ExportDropdown";
import {
  AuthorizationTooltipButton,
  Badge,
  Button,
  ConfirmDialog,
  CopyableText,
  DataTablePage,
  Input,
  Select,
  StatusBadge,
  type DataTableColumn,
} from "@/components/ui";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import type { ListFilter } from "@/types/list-query";
import { EMPTY_PLACEHOLDER } from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { isBizRole } from "@/utils/ops/roles";
import {
  exportResourceCsv,
  runResourceCsvImport,
  type ColumnConfig,
} from "@/utils/resource-csv";
import { logger } from "@/utils/sys/logger";
import { showSuccessToast, showWarningToast } from "@/utils/ui/message-guard";
import { ChannelDetailDrawer } from "./ChannelDetailDrawer";
import { ChannelFormModal } from "./ChannelFormModal";
import {
  formatCostPrice,
  isValidCostPrice,
  normalizeCostPriceInput,
} from "./costPrice";

interface ChannelImportRow extends Record<string, unknown> {
  unique_key: string;
  supplier_code: string;
  channel_code: string;
  name: string;
  path_prefix: string;
  cost_price: string;
  qps_limit: number;
  status: string;
}

interface ChannelExportRow extends Record<string, unknown> {
  supplier_code: string;
  channel_code: string;
  name: string;
  path_prefix: string;
  cost_price: string;
  qps_limit: number;
  status: string;
}

const ALLOWED_STATUSES = new Set(["active", "inactive", "suspended"]);
const CHANNEL_CODE_RE = /^[A-Z][A-Z0-9-]*$/;

function normalizeNumber(value?: string): number {
  if (!value?.trim()) return 0;
  const num = Number(value);
  return Number.isInteger(num) && num >= 0 ? num : Number.NaN;
}

export interface ChannelListTabProps {
  active?: boolean;
  refreshToken?: number;
}

export function ChannelListTab({
  active = true,
  refreshToken = 0,
}: ChannelListTabProps) {
  const { t } = useT();
  const role = useAuthStore((s) => s.info.role);
  const canWrite = isBizRole(role);

  const [loading, setLoading] = useState(false);
  const [rows, setRows] = useState<Channel[]>([]);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [total, setTotal] = useState(0);
  const [keywordInput, setKeywordInput] = useState("");
  const [appliedKeyword, setAppliedKeyword] = useState("");
  const [statusFilter, setStatusFilter] = useState("");
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [hasActiveSupplier, setHasActiveSupplier] = useState(false);

  const [formOpen, setFormOpen] = useState(false);
  const [editingChannel, setEditingChannel] = useState<Channel | null>(null);
  const [detailOpen, setDetailOpen] = useState(false);
  const [activeChannelId, setActiveChannelId] = useState("");
  const [detailReloadToken, setDetailReloadToken] = useState(0);
  const [importOpen, setImportOpen] = useState(false);

  const [deleteTarget, setDeleteTarget] = useState<Channel | null>(null);
  const [batchDeleteIds, setBatchDeleteIds] = useState<string[] | null>(null);
  const [deleting, setDeleting] = useState(false);

  const checkHasActiveSupplier = useCallback(async () => {
    if (!canWrite) {
      setHasActiveSupplier(false);
      return false;
    }
    try {
      const res = await fetchSuppliers({
        status: "active",
        page: 1,
        page_size: 1,
      });
      const ok = (res.pagination?.total ?? 0) > 0;
      setHasActiveSupplier(ok);
      return ok;
    } catch {
      setHasActiveSupplier(false);
      return false;
    }
  }, [canWrite]);

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const filters: ListFilter[] = [];
      if (statusFilter) {
        filters.push({ field: "status", op: "eq", value: statusFilter });
      }
      const res = await fetchChannels({
        q: appliedKeyword || undefined,
        filters: filters.length ? JSON.stringify(filters) : undefined,
        page,
        page_size: pageSize,
      });
      setRows(res.channels ?? []);
      setTotal(res.pagination?.total ?? 0);
      setSelectedIds([]);
    } catch (error) {
      logger.error("Failed to fetch channels:", error);
    } finally {
      setLoading(false);
    }
  }, [appliedKeyword, statusFilter, page, pageSize]);

  useEffect(() => {
    void loadData();
  }, [loadData, refreshToken]);

  useEffect(() => {
    if (active) {
      void checkHasActiveSupplier();
    }
  }, [active, refreshToken, checkHasActiveSupplier]);

  const channelStatusLabel = useCallback(
    (s: string) => {
      if (s === "active") return t("channels.status.active");
      if (s === "inactive") return t("channels.status.inactive");
      if (s === "suspended") return t("channels.status.suspended");
      return s || EMPTY_PLACEHOLDER;
    },
    [t],
  );

  const handleOpenCreate = async () => {
    const ok = await checkHasActiveSupplier();
    if (!ok) {
      showWarningToast(t("channels.messages.createSupplierFirst"));
      return;
    }
    setEditingChannel(null);
    setFormOpen(true);
  };

  const handleOpenImport = async () => {
    const ok = await checkHasActiveSupplier();
    if (!ok) {
      showWarningToast(t("channels.messages.createSupplierFirst"));
      return;
    }
    setImportOpen(true);
  };

  const handleOpenEdit = (row: Channel) => {
    setEditingChannel(row);
    setFormOpen(true);
  };

  const handleFormSuccess = () => {
    void loadData();
    if (detailOpen && activeChannelId) {
      setDetailReloadToken((n) => n + 1);
    }
  };

  const handleConfirmSingleDelete = async () => {
    if (!deleteTarget || deleting) return;
    setDeleting(true);
    try {
      await deleteChannel(deleteTarget.id);
      showSuccessToast(t("common.feedback.deleteSuccess"));
      if (activeChannelId === deleteTarget.id) {
        setDetailOpen(false);
      }
      setDeleteTarget(null);
      await loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to delete channel:", error);
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
        await deleteChannel(id);
      }
      showSuccessToast(t("common.feedback.deleteSuccess"));
      setBatchDeleteIds(null);
      setSelectedIds([]);
      await loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to batch delete channels:", error);
      }
    } finally {
      setDeleting(false);
    }
  };

  const importColumns = useMemo<ColumnConfig[]>(
    () => [
      {
        key: "supplier_code",
        label: t("channels.fields.supplier"),
        required: true,
      },
      {
        key: "channel_code",
        label: t("channels.fields.channelCode"),
        required: true,
      },
      { key: "name", label: t("channels.fields.name"), required: true },
      {
        key: "path_prefix",
        label: t("channels.fields.pathPrefix"),
        required: true,
      },
      {
        key: "cost_price",
        label: t("channels.fields.costPrice"),
        required: false,
      },
      {
        key: "qps_limit",
        label: t("channels.fields.qpsLimit"),
        required: false,
      },
      { key: "status", label: t("common.fields.status"), required: false },
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
    () =>
      rows.map((r) => ({
        unique_key: `${r.supplier_code}:${r.channel_code}`,
      })),
    [rows],
  );

  const validateImportRow = (row: Record<string, string>) => {
    if (!row.supplier_code?.trim()) {
      return { valid: false, reason: t("channels.validation.supplierRequired") };
    }
    const code = row.channel_code?.trim() ?? "";
    if (!code) {
      return {
        valid: false,
        reason: t("channels.validation.channelCodeRequired"),
      };
    }
    if (!CHANNEL_CODE_RE.test(code)) {
      return {
        valid: false,
        reason: t("channels.validation.channelCodePattern"),
      };
    }
    if (!row.name?.trim()) {
      return { valid: false, reason: t("channels.validation.nameRequired") };
    }
    if (!row.path_prefix?.trim()) {
      return {
        valid: false,
        reason: t("channels.validation.pathPrefixRequired"),
      };
    }
    const rawPrice = row.cost_price?.trim();
    if (rawPrice && !isValidCostPrice(rawPrice)) {
      return { valid: false, reason: t("channels.validation.costPricePattern") };
    }
    const rawStatus = row.status?.trim();
    if (rawStatus && !ALLOWED_STATUSES.has(rawStatus.toLowerCase())) {
      return {
        valid: false,
        reason: `${t("common.fields.status")}: ${rawStatus}`,
      };
    }
    const qps = normalizeNumber(row.qps_limit);
    if (Number.isNaN(qps)) {
      return {
        valid: false,
        reason: `${t("channels.fields.qpsLimit")}: ${row.qps_limit}`,
      };
    }
    return { valid: true };
  };

  const transformImportRow = (row: Record<string, string>): ChannelImportRow => {
    const supplierCode = row.supplier_code.trim();
    const rawPrice = row.cost_price?.trim();
    const rawStatus = row.status?.trim().toLowerCase();
    return {
      unique_key: `${supplierCode}:${row.channel_code.trim()}`,
      supplier_code: supplierCode,
      channel_code: row.channel_code.trim(),
      name: row.name.trim(),
      path_prefix: row.path_prefix.trim(),
      cost_price: rawPrice ? normalizeCostPriceInput(rawPrice) : "0.00",
      qps_limit: normalizeNumber(row.qps_limit) || 0,
      status:
        rawStatus && ALLOWED_STATUSES.has(rawStatus) ? rawStatus : "active",
    };
  };

  const resolveSupplierByCode = async (
    supplierCode: string,
    cache: Map<string, Supplier>,
  ): Promise<Supplier | undefined> => {
    const cached = cache.get(supplierCode);
    if (cached) return cached;
    const res = await fetchSuppliers({
      status: "active",
      page: 1,
      page_size: 20,
      q: supplierCode,
    });
    const match = (res.suppliers ?? []).find(
      (item) => item.supplier_code === supplierCode,
    );
    if (match) cache.set(supplierCode, match);
    return match;
  };

  const handleImport = async (items: Record<string, unknown>[]) => {
    const supplierCache = new Map<string, Supplier>();
    await runResourceCsvImport<ChannelImportRow>(items, async (row) => {
      const supplier = await resolveSupplierByCode(
        row.supplier_code,
        supplierCache,
      );
      if (!supplier) {
        throw new Error(`${t("channels.fields.supplier")}: ${row.supplier_code}`);
      }
      const created = await createChannel({
        supplier_id: supplier.id,
        channel_code: row.channel_code,
        name: row.name,
        path_prefix: row.path_prefix,
        cost_price: row.cost_price || "0.00",
        qps_limit: row.qps_limit,
      });
      if (row.status !== "active") {
        await updateChannel(created.id, { status: row.status });
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
    const csvRows: ChannelExportRow[] = targetRows.map((row) => ({
      supplier_code: row.supplier_code,
      channel_code: row.channel_code,
      name: row.name,
      path_prefix: row.path_prefix,
      cost_price: formatCostPrice(row.cost_price),
      qps_limit: row.qps_limit,
      status: row.status,
    }));
    exportResourceCsv(
      csvRows,
      [
        { key: "supplier_code", label: t("channels.fields.supplier") },
        { key: "channel_code", label: t("channels.fields.channelCode") },
        { key: "name", label: t("channels.fields.name") },
        { key: "path_prefix", label: t("channels.fields.pathPrefix") },
        { key: "cost_price", label: t("channels.fields.costPrice") },
        { key: "qps_limit", label: t("channels.fields.qpsLimit") },
        { key: "status", label: t("common.fields.status") },
      ],
      `channels-${command}-${Date.now().toString(36)}.csv`,
    );
  };

  const columns = useMemo<DataTableColumn<Channel>[]>(
    () => [
      {
        id: "channel_code",
        header: t("channels.fields.channelCode"),
        minWidth: 130,
        cell: (row) => <CopyableText value={row.channel_code} />,
      },
      {
        id: "name",
        header: t("channels.fields.name"),
        minWidth: 140,
        accessorKey: "name",
      },
      {
        id: "supplier",
        header: t("channels.fields.supplier"),
        minWidth: 160,
        cell: (row) => `${row.supplier_name} (${row.supplier_code})`,
      },
      {
        id: "path_prefix",
        header: t("channels.fields.pathPrefix"),
        minWidth: 120,
        mono: true,
        accessorKey: "path_prefix",
      },
      {
        id: "cost_price",
        header: t("channels.fields.costPrice"),
        width: 112,
        align: "right",
        mono: true,
        cell: (row) => formatCostPrice(row.cost_price),
      },
      {
        id: "qps_limit",
        header: t("channels.fields.qpsLimit"),
        width: 112,
        align: "right",
        cell: (row) =>
          row.qps_limit === 0 ? (
            <Badge variant="outline">{t("channels.fields.qpsUnlimited")}</Badge>
          ) : (
            <span className="font-mono-tech tabular-nums">{row.qps_limit}</span>
          ),
      },
      {
        id: "status",
        header: t("common.fields.status"),
        width: 100,
        cell: (row) => (
          <StatusBadge
            status={
              row.status === "active"
                ? "ACTIVE"
                : row.status === "suspended"
                  ? "SUSPENDED"
                  : "DISABLED"
            }
            label={channelStatusLabel(row.status)}
          />
        ),
      },
      {
        id: "operations",
        header: t("common.fields.operations"),
        width: canWrite ? 176 : 88,
        align: "right",
        cell: (row) => (
          <div className="inline-flex items-center justify-end gap-1">
            <Button
              type="button"
              variant="link"
              size="xs"
              onClick={() => {
                setActiveChannelId(row.id);
                setDetailOpen(true);
              }}
            >
              {t("common.actions.details")}
            </Button>
            {canWrite && (
              <>
                <Button
                  type="button"
                  variant="link"
                  size="xs"
                  onClick={() => handleOpenEdit(row)}
                >
                  {t("channels.actions.edit")}
                </Button>
                <Button
                  type="button"
                  variant="link"
                  size="xs"
                  className="text-danger-fg hover:text-danger-fg hover:bg-danger-fg/10"
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
    [t, canWrite, channelStatusLabel],
  );

  const handleSearch = () => {
    setPage(1);
    setAppliedKeyword(keywordInput.trim());
  };

  const hasActiveSearch = Boolean(appliedKeyword || statusFilter);

  return (
    <>
      <DataTablePage
        columns={columns}
        data={rows}
        getRowId={(row) => row.id}
        loading={loading}
        emptyText={
          hasActiveSearch
            ? t("channels.empty.filtered")
            : t("channels.empty.default")
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
        testId="channel-list-table"
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
                data-testid="channel-search-input"
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
              aria-label={t("common.fields.status")}
              className="w-28"
              options={[
                { value: "", label: t("common.all"), text: t("common.all") },
                {
                  value: "active",
                  label: t("channels.status.active"),
                  text: t("channels.status.active"),
                },
                {
                  value: "disabled",
                  label: t("channels.status.inactive"),
                  text: t("channels.status.inactive"),
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
                  data-testid="channel-batch-delete-btn"
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
              <AuthorizationTooltipButton
                type="button"
                variant="outline"
                size="md"
                disabled={!hasActiveSupplier}
                disabledReason={t("channels.messages.createSupplierFirst")}
                onClick={() => void handleOpenImport()}
              >
                <Upload className="h-3.5 w-3.5" />
                {t("common.actions.import")}
              </AuthorizationTooltipButton>
            )}
            <ExportDropdown
              items={exportItems}
              disabled={rows.length === 0}
              onCommand={handleExportCommand}
            />
            {canWrite && (
              <AuthorizationTooltipButton
                type="button"
                variant="primary"
                size="md"
                disabled={!hasActiveSupplier}
                disabledReason={t("channels.messages.createSupplierFirst")}
                onClick={() => void handleOpenCreate()}
                data-testid="channel-create-btn"
              >
                <Plus className="h-3.5 w-3.5" />
                {t("channels.actions.add")}
              </AuthorizationTooltipButton>
            )}
          </>
        }
      />

      <ChannelFormModal
        open={formOpen}
        channel={editingChannel}
        onClose={() => setFormOpen(false)}
        onSuccess={handleFormSuccess}
      />

      <ChannelDetailDrawer
        open={detailOpen}
        channelId={activeChannelId}
        reloadToken={detailReloadToken}
        onClose={() => setDetailOpen(false)}
        onEdit={handleOpenEdit}
      />

      <CsvImportModal
        open={importOpen}
        onClose={() => setImportOpen(false)}
        title={t("common.actions.import")}
        columns={importColumns}
        formatHint={importFormatHint}
        uniqueKey="unique_key"
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
        title={t("common.warning")}
        description={
          deleteTarget
            ? t("channels.messages.deleteConfirm", { name: deleteTarget.name })
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
        description={t("channels.messages.deleteConfirm", {
          name: `${batchDeleteIds?.length ?? 0}`,
        })}
        confirmLabel={t("common.confirm")}
        cancelLabel={t("common.cancel")}
        loading={deleting}
      />
    </>
  );
}
