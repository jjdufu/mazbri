import { useCallback, useEffect, useMemo, useState } from "react";
import { CalendarPlus, Download, Plus, RefreshCw, Trash2 } from "lucide-react";
import {
  createBillAdjustment,
  deleteBill,
  fetchBillAdjustments,
  fetchBillById,
  fetchBillList,
  generateBill,
  generateBillAsync,
  updateBillStatus,
  voidBillAdjustment,
  type BillAdjustment,
} from "@/api/billing";
import { fetchChannels, type Channel } from "@/api/route-management";
import { fetchSuppliers, type Supplier } from "@/api/supplier";
import { fetchTenantList } from "@/api/tenant";
import {
  ExportDropdown,
  type ExportDropdownItem,
} from "@/components/common/ExportDropdown";
import {
  Badge,
  Button,
  ConfirmDialog,
  CopyableText,
  DataTablePage,
  Drawer,
  EmptyState,
  EntityDetailCard,
  FormField,
  FormModal,
  Input,
  Select,
  StatusBadge,
  Textarea,
  type DataTableColumn,
  type SelectOptionItem,
} from "@/components/ui";
import { StructuredFilterBar } from "@/domains/ops/shared/StructuredFilterBar";
import { billsCatalog } from "@/list-catalog/common-resources";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import type {
  Bill,
  BillLineItem,
  BillListSummary,
  GenerateBillRequest,
} from "@/types/api/billing";
import type { Tenant } from "@/types/api/tenant";
import type { ListFilter } from "@/types/list-query";
import { downloadBinary, packageTarArchive } from "@/utils/archive";
import { downloadCsv, serializeRawCsv } from "@/utils/csv";
import {
  EMPTY_PLACEHOLDER,
  formatBytes,
  formatDate,
  formatDisplayDateYmd,
  formatMoney,
} from "@/utils/format";
import { getHttpErrorRawMessage } from "@/utils/http/error";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { isBizRole } from "@/utils/ops/roles";
import { logger } from "@/utils/sys/logger";
import {
  showErrorToast,
  showSuccessToast,
  showWarningToast,
} from "@/utils/ui/message-guard";
import {
  canMarkOverdue,
  canMarkPaid,
  getStatusLabel,
  statusBadgeKey,
} from "../helpers/statusHelpers";

export interface BillListPanelProps {
  active?: boolean;
}

const AMOUNT_DEC_RE = /^(?:0|[1-9]\d{0,11})(?:\.\d{1,4})?$/;

function formatDateKey(date?: string | null): string {
  if (!date) return "unknown-date";
  const parsed = new Date(date);
  if (Number.isNaN(parsed.getTime())) return "unknown-date";
  return formatDisplayDateYmd(parsed.getTime()).replace(/-/g, "");
}

function toIsoAtBoundary(date: string, endOfDay = false): string {
  const suffix = endOfDay ? "23:59:59.999" : "00:00:00.000";
  return new Date(`${date}T${suffix}+08:00`).toISOString();
}

export function BillListPanel({ active: _active = true }: BillListPanelProps) {
  const { t } = useT();
  const role = useAuthStore((s) => s.info.role);
  const canWrite = isBizRole(role);

  const [loading, setLoading] = useState(false);
  const [bills, setBills] = useState<Bill[]>([]);
  const [summary, setSummary] = useState<BillListSummary | null>(null);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(10);
  const [total, setTotal] = useState(0);

  const [qInput, setQInput] = useState("");
  const [appliedQ, setAppliedQ] = useState("");
  const [listFilters, setListFilters] = useState<ListFilter[]>([]);
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [exporting, setExporting] = useState(false);

  // Detail Drawer
  const [detailOpen, setDetailOpen] = useState(false);
  const [detailLoading, setDetailLoading] = useState(false);
  const [currentBill, setCurrentBill] = useState<Bill | null>(null);
  const [billItems, setBillItems] = useState<BillLineItem[]>([]);
  const [adjustments, setAdjustments] = useState<BillAdjustment[]>([]);
  const [adjustmentsLoading, setAdjustmentsLoading] = useState(false);

  // Adjustment Modal
  const [adjOpen, setAdjOpen] = useState(false);
  const [adjSaving, setAdjSaving] = useState(false);
  const [adjType, setAdjType] = useState("credit");
  const [adjAmountStr, setAdjAmountStr] = useState("");
  const [adjReason, setAdjReason] = useState("");
  const [adjErrors, setAdjErrors] = useState<{
    amount?: string;
    reason?: string;
  }>({});
  const [voidTarget, setVoidTarget] = useState<BillAdjustment | null>(null);
  const [voiding, setVoiding] = useState(false);

  // Generate Bill Modal
  const [genOpen, setGenOpen] = useState(false);
  const [generating, setGenerating] = useState(false);
  const [billKind, setBillKind] = useState<"customer" | "upstream">("customer");
  const [genTenantCode, setGenTenantCode] = useState("");
  const [upstreamScopeType, setUpstreamScopeType] = useState<
    "channel" | "supplier"
  >("channel");
  const [upstreamScopeId, setUpstreamScopeId] = useState("");
  const [periodStart, setPeriodStart] = useState("");
  const [periodEnd, setPeriodEnd] = useState("");
  const [genErrors, setGenErrors] = useState<{
    tenant_code?: string;
    upstream_scope_id?: string;
    period?: string;
  }>({});

  const [tenants, setTenants] = useState<Tenant[]>([]);
  const [channels, setChannels] = useState<Channel[]>([]);
  const [suppliers, setSuppliers] = useState<Supplier[]>([]);
  const [catalogLoading, setCatalogLoading] = useState(false);

  // Status update & delete confirms
  const [statusConfirm, setStatusConfirm] = useState<{
    row: Bill;
    status: "paid" | "overdue" | "processing" | "cancelled";
  } | null>(null);
  const [statusUpdating, setStatusUpdating] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<Bill | null>(null);
  const [batchDeleteIds, setBatchDeleteIds] = useState<string[] | null>(null);
  const [deleting, setDeleting] = useState(false);

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const res = await fetchBillList({
        q: appliedQ || undefined,
        page,
        page_size: pageSize,
        filters:
          listFilters.length > 0 ? JSON.stringify(listFilters) : undefined,
      });
      setBills(res.items ?? []);
      setSummary(res.summary ?? null);
      setTotal(res.pagination?.total ?? 0);
      setSelectedIds([]);
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to load bills:", error);
      }
    } finally {
      setLoading(false);
    }
  }, [appliedQ, page, pageSize, listFilters]);

  useEffect(() => {
    void loadData();
  }, [loadData]);

  const getBillKindLabel = useCallback(
    (kind?: string | null) => {
      if (kind === "upstream") return t("billing.billKinds.upstream");
      return t("billing.billKinds.customer");
    },
    [t],
  );

  const getBillDisplayName = useCallback(
    (bill: Bill | null) => {
      if (!bill) return EMPTY_PLACEHOLDER;
      if (bill.bill_kind === "upstream") {
        return (
          bill.party_name || bill.tenant_name || t("billing.billKinds.upstream")
        );
      }
      return bill.tenant_name || bill.party_name || EMPTY_PLACEHOLDER;
    },
    [t],
  );

  const buildBillDetailCsvRows = useCallback(
    (bill: Bill, items: BillLineItem[]) => [
      [t("billing.labels.billKind"), getBillKindLabel(bill.bill_kind)],
      [t("billing.labels.tenantName"), bill.tenant_name],
      [t("billing.labels.tenantCode"), bill.tenant_code],
      [t("billing.labels.tenantId"), bill.tenant_id],
      [t("billing.labels.billId"), bill.id],
      [t("billing.labels.periodStart"), formatDate(bill.period_start)],
      [t("billing.labels.periodEnd"), formatDate(bill.period_end)],
      [t("billing.labels.total"), bill.total],
      [t("billing.labels.totalCalls"), bill.total_calls ?? 0],
      [
        t("billing.labels.dataVolume"),
        formatBytes(bill.total_data_bytes ?? 0),
      ],
      [],
      [
        t("billing.itemColumns.description"),
        t("billing.itemColumns.quantity"),
        t("billing.itemColumns.totalUnits"),
        t("billing.itemColumns.unitPrice"),
        t("billing.itemColumns.total"),
      ],
      ...items.map((item) => [
        item.description,
        item.quantity,
        item.total_units,
        item.unit_price,
        item.total,
      ]),
    ],
    [getBillKindLabel, t],
  );

  const buildBillCsvName = (bill: Bill) =>
    `${bill.party_code || bill.tenant_code || bill.tenant_id}-${formatDateKey(bill.period_start)}-${formatDateKey(bill.period_end)}-${bill.id}.csv`;

  const selectedRows = useMemo(
    () => bills.filter((row) => selectedIds.includes(row.id)),
    [bills, selectedIds],
  );

  const exportItems = useMemo<ExportDropdownItem[]>(
    () => [
      {
        command: "page",
        label: t("billing.actions.exportPage"),
        disabled: bills.length === 0,
      },
      {
        command: "selected",
        label: t("billing.actions.exportSelected", {
          count: selectedRows.length,
        }),
        disabled: selectedRows.length === 0,
      },
    ],
    [bills.length, selectedRows.length, t],
  );

  const handleExportCommand = async (command: string) => {
    const targetRows = command === "selected" ? selectedRows : bills;
    if (targetRows.length === 0) {
      showWarningToast(t("billing.messages.noExport"));
      return;
    }
    setExporting(true);
    try {
      const details = await Promise.all(
        targetRows.map((row) => fetchBillById(row.id)),
      );
      const encoder = new TextEncoder();
      const files = details.map(({ bill, items }) => ({
        name: buildBillCsvName(bill),
        content: encoder.encode(
          `${serializeRawCsv(buildBillDetailCsvRows(bill, items))}\n`,
        ),
      }));
      const prefix = command === "selected" ? "bills-selected" : "bills-page";
      const packaged = await packageTarArchive(
        files,
        `${prefix}-${Date.now().toString(36)}.tar.gz`,
      );
      downloadBinary(packaged.filename, packaged.data, packaged.mimeType);
    } catch (error) {
      logger.error("Failed to export bill archive:", error);
      showErrorToast(t("billing.messages.exportDetailFailed"));
    } finally {
      setExporting(false);
    }
  };

  const handleExportDetail = () => {
    if (!currentBill) return;
    const rows = buildBillDetailCsvRows(currentBill, billItems);
    downloadCsv(`\uFEFF${serializeRawCsv(rows)}`, buildBillCsvName(currentBill));
  };

  const loadBillAdjustments = async (billId: string) => {
    setAdjustmentsLoading(true);
    try {
      const res = await fetchBillAdjustments(billId);
      setAdjustments(res.items ?? []);
    } catch (error) {
      logger.error("Failed to load bill adjustments:", error);
      setAdjustments([]);
    } finally {
      setAdjustmentsLoading(false);
    }
  };

  const handleViewDetail = async (row: Bill) => {
    if (!row.id) return;
    setDetailOpen(true);
    setCurrentBill(row);
    setBillItems([]);
    setAdjustments([]);
    setDetailLoading(true);
    try {
      const detail = await fetchBillById(row.id);
      setCurrentBill(detail.bill);
      setBillItems(detail.items ?? []);
      await loadBillAdjustments(detail.bill.id);
    } catch (error) {
      logger.error("Failed to load bill detail:", error);
      showErrorToast(t("billing.messages.loadBillDetailFailed"));
    } finally {
      setDetailLoading(false);
    }
  };

  const handleConfirmStatus = async () => {
    if (!statusConfirm || statusUpdating) return;
    setStatusUpdating(true);
    try {
      await updateBillStatus(statusConfirm.row.id, statusConfirm.status);
      const updatedId = statusConfirm.row.id;
      const updatedRow = statusConfirm.row;
      setStatusConfirm(null);
      await loadData();
      if (currentBill?.id === updatedId) {
        await handleViewDetail(updatedRow);
      }
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to update bill status:", error);
      }
    } finally {
      setStatusUpdating(false);
    }
  };

  const handleConfirmSingleDelete = async () => {
    if (!deleteTarget || deleting) return;
    setDeleting(true);
    try {
      await deleteBill(deleteTarget.id);
      setDeleteTarget(null);
      void loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to delete bill:", error);
      }
    } finally {
      setDeleting(false);
    }
  };

  const handleConfirmBatchDelete = async () => {
    if (!batchDeleteIds || batchDeleteIds.length === 0 || deleting) return;
    setDeleting(true);
    let deletedCount = 0;
    try {
      for (const id of batchDeleteIds) {
        try {
          await deleteBill(id);
          deletedCount += 1;
        } catch (error) {
          if (isMfaStepUpCancelled(error)) break;
          logger.error(`Failed to delete bill ${id}:`, error);
        }
      }
      if (deletedCount > 0) {
        setSelectedIds([]);
        void loadData();
      }
      setBatchDeleteIds(null);
    } finally {
      setDeleting(false);
    }
  };

  const handleCreateAdjustment = async () => {
    if (!currentBill || adjSaving) return;
    const nextErrors: { amount?: string; reason?: string } = {};
    const trimmedAmt = adjAmountStr.trim();
    const numericAmt = Number(trimmedAmt);
    if (
      !trimmedAmt ||
      !AMOUNT_DEC_RE.test(trimmedAmt) ||
      !Number.isFinite(numericAmt) ||
      numericAmt < 0.0001
    ) {
      nextErrors.amount = t("billing.adjustments.amountPlaceholder");
    }
    if (!adjReason.trim()) {
      nextErrors.reason = t("billing.adjustments.reasonPlaceholder");
    }
    setAdjErrors(nextErrors);
    if (Object.keys(nextErrors).length > 0) return;

    setAdjSaving(true);
    try {
      await createBillAdjustment(currentBill.id, {
        adjustment_type: adjType,
        amount: trimmedAmt,
        reason: adjReason.trim(),
      });
      setAdjOpen(false);
      await loadBillAdjustments(currentBill.id);
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to create bill adjustment:", error);
      }
    } finally {
      setAdjSaving(false);
    }
  };

  const handleConfirmVoidAdjustment = async () => {
    if (!voidTarget || voiding) return;
    setVoiding(true);
    try {
      await voidBillAdjustment(voidTarget.id);
      setVoidTarget(null);
      if (currentBill) {
        await loadBillAdjustments(currentBill.id);
      }
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to void bill adjustment:", error);
      }
    } finally {
      setVoiding(false);
    }
  };

  // Load selectors for Generate Bill Modal
  const loadGenerateCatalog = useCallback(
    async (
      kind: "customer" | "upstream",
      scopeType: "channel" | "supplier",
    ) => {
      setCatalogLoading(true);
      try {
        if (kind === "customer" && tenants.length === 0) {
          const res = await fetchTenantList({ page: 1, page_size: 100 });
          setTenants(res.tenants ?? []);
        } else if (kind === "upstream" && scopeType === "channel" && channels.length === 0) {
          const res = await fetchChannels({ page: 1, page_size: 100 });
          setChannels(res.channels ?? []);
        } else if (kind === "upstream" && scopeType === "supplier" && suppliers.length === 0) {
          const res = await fetchSuppliers({ page: 1, page_size: 100 });
          setSuppliers(res.suppliers ?? []);
        }
      } catch (error) {
        logger.error("Failed to load bill generation catalog:", error);
      } finally {
        setCatalogLoading(false);
      }
    },
    [tenants.length, channels.length, suppliers.length],
  );

  const handleOpenGenerate = () => {
    setBillKind("customer");
    setGenTenantCode("");
    setUpstreamScopeType("channel");
    setUpstreamScopeId("");
    setPeriodStart("");
    setPeriodEnd("");
    setGenErrors({});
    setGenOpen(true);
    void loadGenerateCatalog("customer", "channel");
  };

  const resolveGenerateError = (error: unknown): string => {
    const message =
      getHttpErrorRawMessage(error) ??
      (error instanceof Error ? error.message : String(error ?? ""));
    const normalized = message.toLowerCase();
    if (normalized.includes("already exists")) {
      return t("billing.messages.billAlreadyExists");
    }
    if (
      normalized.includes("no upstream request logs") ||
      normalized.includes("no billable upstream request logs")
    ) {
      return t("billing.messages.noUpstreamLogs");
    }
    if (normalized.includes("period start")) {
      return t("billing.messages.invalidPeriod");
    }
    return t("billing.messages.generateFailed");
  };

  const handleSubmitGenerate = async () => {
    if (generating) return;
    const nextErrors: {
      tenant_code?: string;
      upstream_scope_id?: string;
      period?: string;
    } = {};
    if (billKind === "customer" && !genTenantCode.trim()) {
      nextErrors.tenant_code = t("billing.validation.tenantRequired");
    }
    if (billKind === "upstream" && !upstreamScopeId.trim()) {
      nextErrors.upstream_scope_id = t(
        "billing.validation.upstreamScopeRequired",
      );
    }
    if (!periodStart || !periodEnd) {
      nextErrors.period = t("billing.validation.periodRangeRequired");
    } else if (periodStart > periodEnd) {
      nextErrors.period = t("billing.messages.invalidPeriod");
    }
    setGenErrors(nextErrors);
    if (Object.keys(nextErrors).length > 0) return;

    const payload: GenerateBillRequest = {
      bill_kind: billKind,
      period_start: toIsoAtBoundary(periodStart, false),
      period_end: toIsoAtBoundary(periodEnd, true),
    };
    if (billKind === "customer") {
      payload.tenant_code = genTenantCode;
    } else {
      payload.upstream_scope_type = upstreamScopeType;
      payload.upstream_scope_id = upstreamScopeId;
    }

    setGenerating(true);
    try {
      if (billKind === "upstream") {
        await generateBillAsync(payload);
      } else {
        await generateBill(payload);
      }
      showSuccessToast(t("billing.messages.generateSuccess"));
      setGenOpen(false);
      void loadData();
    } catch (error) {
      if (isMfaStepUpCancelled(error)) return;
      logger.error("Failed to generate bill:", error);
      showErrorToast(resolveGenerateError(error));
    } finally {
      setGenerating(false);
    }
  };

  const hasActiveSearch = Boolean(appliedQ) || listFilters.length > 0;
  const emptyText = useMemo(() => {
    const hint = summary?.status_hint;
    if (hint === "filtered_empty" || (hasActiveSearch && bills.length === 0)) {
      return t("billing.empty.filteredEmpty");
    }
    if (hint === "pending_generation") {
      return t("billing.empty.pendingGeneration");
    }
    if (hint === "no_billing_cycle") {
      return t("billing.empty.noBillingCycle");
    }
    return t("billing.empty.noData");
  }, [summary?.status_hint, hasActiveSearch, bills.length, t]);

  const tenantOptions = useMemo<SelectOptionItem[]>(
    () =>
      tenants.map((item) => ({
        value: item.tenant_code,
        text: `${item.name} (${item.tenant_code})`,
        label: `${item.name} (${item.tenant_code})`,
      })),
    [tenants],
  );

  const channelOptions = useMemo<SelectOptionItem[]>(
    () =>
      channels.map((item) => ({
        value: item.id,
        text: `${item.name} (${item.channel_code})`,
        label: `${item.name} (${item.channel_code})`,
      })),
    [channels],
  );

  const supplierOptions = useMemo<SelectOptionItem[]>(
    () =>
      suppliers.map((item) => ({
        value: item.id,
        text: `${item.name} (${item.supplier_code})`,
        label: `${item.name} (${item.supplier_code})`,
      })),
    [suppliers],
  );

  const columns = useMemo<DataTableColumn<Bill>[]>(
    () => [
      {
        id: "tenant_name",
        header: t("billing.columns.tenantName"),
        minWidth: 140,
        cell: (row) => (
          <span className="font-medium text-foreground">
            {getBillDisplayName(row)}
          </span>
        ),
      },
      {
        id: "tenant_code",
        header: t("billing.columns.tenantCode"),
        width: 140,
        cell: (row) => {
          const code = row.tenant_code || row.party_code;
          return code ? <CopyableText value={code} mono /> : EMPTY_PLACEHOLDER;
        },
      },
      {
        id: "bill_kind",
        header: t("billing.columns.billKind"),
        width: 100,
        cell: (row) => (
          <Badge
            variant={row.bill_kind === "upstream" ? "warning" : "success"}
          >
            {getBillKindLabel(row.bill_kind)}
          </Badge>
        ),
      },
      {
        id: "period_start",
        header: t("billing.columns.periodStart"),
        width: 112,
        mono: true,
        cell: (row) => formatDate(row.period_start),
      },
      {
        id: "period_end",
        header: t("billing.columns.periodEnd"),
        width: 112,
        mono: true,
        cell: (row) => formatDate(row.period_end),
      },
      {
        id: "total",
        header: t("billing.columns.total"),
        width: 120,
        align: "right",
        mono: true,
        cell: (row) => (
          <span className="font-medium">{formatMoney(row.total)}</span>
        ),
      },
      {
        id: "status",
        header: t("billing.columns.status"),
        width: 104,
        cell: (row) => (
          <StatusBadge
            status={statusBadgeKey(row.status)}
            label={getStatusLabel(row.status)}
          />
        ),
      },
      {
        id: "operations",
        header: t("common.fields.operations"),
        width: 224,
        align: "right",
        cell: (row) => (
          <div
            className="inline-flex items-center justify-end gap-1"
            onClick={(e) => e.stopPropagation()}
          >
            <Button
              type="button"
              variant="link"
              size="xs"
              onClick={() => void handleViewDetail(row)}
            >
              {t("common.actions.details")}
            </Button>
            {canWrite && canMarkPaid(row.status) && (
              <Button
                type="button"
                variant="link"
                size="xs"
                className="text-success-fg hover:text-success-fg"
                onClick={() => setStatusConfirm({ row, status: "paid" })}
              >
                {t("billing.actions.markPaid")}
              </Button>
            )}
            {canWrite && canMarkOverdue(row.status) && (
              <Button
                type="button"
                variant="link"
                size="xs"
                className="text-warning-fg hover:text-warning-fg"
                onClick={() => setStatusConfirm({ row, status: "overdue" })}
              >
                {t("billing.actions.markOverdue")}
              </Button>
            )}
            {canWrite && (
              <Button
                type="button"
                variant="link"
                size="xs"
                className="text-danger-fg hover:text-danger-fg"
                onClick={() => setDeleteTarget(row)}
              >
                {t("common.actions.delete")}
              </Button>
            )}
          </div>
        ),
      },
    ],
    [canWrite, getBillDisplayName, getBillKindLabel, t],
  );

  return (
    <div className="space-y-4 min-w-0" data-testid="billing-bills-panel">
      <DataTablePage<Bill>
        columns={columns}
        data={bills}
        getRowId={(row) => row.id}
        loading={loading}
        emptyText={emptyText}
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
        filters={
          <StructuredFilterBar
            q={qInput}
            onQChange={setQInput}
            filters={listFilters}
            onFiltersChange={(next) => {
              setListFilters(next);
              setPage(1);
            }}
            catalog={billsCatalog}
            onSearch={() => {
              setAppliedQ(qInput.trim());
              setPage(1);
            }}
          />
        }
        batchActions={
          canWrite
            ? (ids) => (
                <Button
                  type="button"
                  variant="danger"
                  size="sm"
                  onClick={() => setBatchDeleteIds(ids)}
                  data-testid="bills-batch-delete-btn"
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
              disabled={loading}
            >
              <RefreshCw
                className={loading ? "h-3.5 w-3.5 animate-spin" : "h-3.5 w-3.5"}
              />
              {t("common.actions.refresh")}
            </Button>
            <ExportDropdown
              items={exportItems}
              label={t("billing.actions.export")}
              disabled={bills.length === 0 || exporting}
              loading={exporting}
              onCommand={(cmd) => void handleExportCommand(cmd)}
            />
            {canWrite && (
              <Button
                type="button"
                variant="primary"
                size="md"
                onClick={handleOpenGenerate}
                data-testid="bills-generate-btn"
              >
                <CalendarPlus className="h-3.5 w-3.5" />
                {t("billing.actions.generate")}
              </Button>
            )}
          </>
        }
      />

      {/* Bill Detail Drawer */}
      <Drawer
        open={detailOpen}
        onClose={() => setDetailOpen(false)}
        title={t("billing.detail.title")}
        size="lg"
        testId="bill-detail-drawer"
      >
        {detailLoading && !currentBill ? (
          <div className="py-12 text-center text-xs text-muted-foreground">
            {t("layout.loading")}
          </div>
        ) : currentBill ? (
          <div className="space-y-4">
            <div className="flex justify-end">
              <Button
                type="button"
                variant="primary"
                size="sm"
                onClick={handleExportDetail}
              >
                <Download className="h-3.5 w-3.5" />
                {t("billing.detail.download")}
              </Button>
            </div>

            <EntityDetailCard
              title={getBillDisplayName(currentBill)}
              actions={
                <span className="font-mono-tech text-sm font-semibold tabular-nums text-foreground">
                  {formatMoney(currentBill.total)}
                </span>
              }
            >
              <div className="grid grid-cols-1 gap-px overflow-hidden rounded-md border border-border bg-border sm:grid-cols-2">
                {[
                  {
                    label: t("billing.labels.billId"),
                    value: (
                      <span className="font-mono-tech">{currentBill.id}</span>
                    ),
                  },
                  {
                    label: t("billing.labels.billKind"),
                    value: getBillKindLabel(currentBill.bill_kind),
                  },
                  {
                    label: t("billing.labels.period"),
                    value: (
                      <span className="font-mono-tech">
                        {formatDate(currentBill.period_start)} ~{" "}
                        {formatDate(currentBill.period_end)}
                      </span>
                    ),
                  },
                  {
                    label: t("billing.labels.dueDate"),
                    value: formatDate(currentBill.due_date),
                  },
                  {
                    label: t("billing.labels.totalCalls"),
                    value: (
                      <span className="font-mono-tech tabular-nums">
                        {(currentBill.total_calls ?? 0).toLocaleString()}
                      </span>
                    ),
                  },
                  {
                    label: t("billing.labels.dataVolume"),
                    value: (
                      <span className="font-mono-tech">
                        {formatBytes(currentBill.total_data_bytes ?? 0)}
                      </span>
                    ),
                  },
                  {
                    label: t("common.fields.createdAt"),
                    value: formatDate(currentBill.created_at),
                  },
                  {
                    label: t("common.fields.updatedAt"),
                    value: formatDate(currentBill.updated_at),
                  },
                ].map((item, idx) => (
                  <div
                    key={idx}
                    className="flex items-center justify-between gap-2 bg-card px-3 py-2 text-xs"
                  >
                    <span className="text-muted-foreground">{item.label}</span>
                    <span className="font-medium text-foreground">
                      {item.value}
                    </span>
                  </div>
                ))}
              </div>
            </EntityDetailCard>

            {/* Line Items */}
            <EntityDetailCard
              title={`${t("billing.detail.itemsTitle")} (${billItems.length})`}
              bodyClassName="p-0"
            >
              {billItems.length === 0 ? (
                <div className="p-4">
                  <EmptyState title={t("billing.detail.emptyItems")} />
                </div>
              ) : (
                <div className="overflow-x-auto">
                  <table className="w-full border-collapse text-xs text-foreground">
                    <thead>
                      <tr className="border-b border-border bg-muted/45 text-muted-foreground">
                        <th className="h-8 px-3 text-left font-medium">
                          {t("billing.itemColumns.description")}
                        </th>
                        <th className="h-8 px-2.5 text-right font-medium">
                          {t("billing.itemColumns.quantity")}
                        </th>
                        <th className="h-8 px-2.5 text-right font-medium">
                          {t("billing.itemColumns.totalUnits")}
                        </th>
                        <th className="h-8 px-2.5 text-right font-medium">
                          {t("billing.itemColumns.unitPrice")}
                        </th>
                        <th className="h-8 px-3 text-right font-medium">
                          {t("billing.itemColumns.total")}
                        </th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-border-subtle">
                      {billItems.map((item) => (
                        <tr key={item.item_id}>
                          <td className="px-3 py-2">{item.description}</td>
                          <td className="px-2.5 py-2 text-right font-mono-tech tabular-nums">
                            {item.quantity}
                          </td>
                          <td className="px-2.5 py-2 text-right font-mono-tech tabular-nums">
                            {item.total_units}
                          </td>
                          <td className="px-2.5 py-2 text-right font-mono-tech tabular-nums">
                            {formatMoney(item.unit_price)}
                          </td>
                          <td className="px-3 py-2 text-right font-mono-tech tabular-nums font-medium">
                            {formatMoney(item.total)}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </EntityDetailCard>

            {/* Bill Adjustments */}
            <EntityDetailCard
              title={t("billing.adjustments.title")}
              bodyClassName="p-0"
              actions={
                canWrite ? (
                  <Button
                    type="button"
                    variant="primary"
                    size="sm"
                    onClick={() => {
                      setAdjType("credit");
                      setAdjAmountStr("");
                      setAdjReason("");
                      setAdjErrors({});
                      setAdjOpen(true);
                    }}
                  >
                    <Plus className="h-3.5 w-3.5" />
                    {t("billing.adjustments.add")}
                  </Button>
                ) : undefined
              }
            >
              {adjustmentsLoading ? (
                <div className="py-6 text-center text-xs text-muted-foreground">
                  {t("layout.loading")}
                </div>
              ) : adjustments.length === 0 ? (
                <div className="p-4">
                  <EmptyState title={t("billing.adjustments.empty")} />
                </div>
              ) : (
                <div className="overflow-x-auto">
                  <table className="w-full border-collapse text-xs text-foreground">
                    <thead>
                      <tr className="border-b border-border bg-muted/45 text-muted-foreground">
                        <th className="h-8 px-3 text-left font-medium">
                          {t("billing.adjustments.type")}
                        </th>
                        <th className="h-8 px-2.5 text-right font-medium">
                          {t("billing.adjustments.amount")}
                        </th>
                        <th className="h-8 px-2.5 text-left font-medium">
                          {t("billing.adjustments.reason")}
                        </th>
                        <th className="h-8 px-2.5 text-left font-medium">
                          {t("billing.adjustments.status")}
                        </th>
                        <th className="h-8 px-2.5 text-left font-medium">
                          {t("common.fields.createdAt")}
                        </th>
                        <th className="h-8 px-3 text-right font-medium">
                          {t("common.fields.operations")}
                        </th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-border-subtle">
                      {adjustments.map((adj) => (
                        <tr key={adj.id}>
                          <td className="px-3 py-2">{adj.adjustment_type}</td>
                          <td className="px-2.5 py-2 text-right font-mono-tech tabular-nums">
                            {formatMoney(adj.amount)}
                          </td>
                          <td className="px-2.5 py-2">{adj.reason}</td>
                          <td className="px-2.5 py-2">
                            <Badge
                              variant={
                                adj.status === "void" ? "neutral" : "success"
                              }
                            >
                              {adj.status === "void"
                                ? t("billing.adjustments.statuses.void")
                                : t("billing.adjustments.statuses.posted")}
                            </Badge>
                          </td>
                          <td className="px-2.5 py-2 font-mono-tech">
                            {formatDate(adj.created_at)}
                          </td>
                          <td className="px-3 py-2 text-right">
                            {canWrite && adj.status === "posted" ? (
                              <Button
                                type="button"
                                variant="link"
                                size="xs"
                                className="text-danger-fg hover:text-danger-fg"
                                onClick={() => setVoidTarget(adj)}
                              >
                                {t("billing.adjustments.void")}
                              </Button>
                            ) : (
                              <span className="text-muted-foreground">
                                {EMPTY_PLACEHOLDER}
                              </span>
                            )}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </EntityDetailCard>
          </div>
        ) : null}
      </Drawer>

      {/* Create Adjustment Modal (FormModal size="sm", label-top, U2 exact string) */}
      <FormModal
        open={adjOpen}
        onClose={() => setAdjOpen(false)}
        onSubmit={handleCreateAdjustment}
        title={t("billing.adjustments.addTitle")}
        size="sm"
        submitLabel={t("common.confirm")}
        submitting={adjSaving}
      >
        <FormField label={t("billing.adjustments.type")} required>
          <Select
            value={adjType}
            onChange={(e) => setAdjType(e.target.value)}
            options={[
              {
                value: "credit",
                label: t("billing.adjustments.types.credit"),
                text: t("billing.adjustments.types.credit"),
              },
              {
                value: "debit",
                label: t("billing.adjustments.types.debit"),
                text: t("billing.adjustments.types.debit"),
              },
              {
                value: "correction",
                label: t("billing.adjustments.types.correction"),
                text: t("billing.adjustments.types.correction"),
              },
              {
                value: "other",
                label: t("billing.adjustments.types.other"),
                text: t("billing.adjustments.types.other"),
              },
            ]}
            className="w-full"
          />
        </FormField>

        <FormField
          label={t("billing.adjustments.amount")}
          htmlFor="bill-adj-amount"
          required
          error={adjErrors.amount}
        >
          <Input
            id="bill-adj-amount"
            type="text"
            inputMode="decimal"
            value={adjAmountStr}
            onChange={(e) => {
              setAdjAmountStr(e.target.value);
              if (adjErrors.amount) {
                setAdjErrors((prev) => ({ ...prev, amount: undefined }));
              }
            }}
            placeholder={t("billing.adjustments.amountPlaceholder")}
            className="font-mono-tech tabular-nums"
          />
        </FormField>

        <FormField
          label={t("billing.adjustments.reason")}
          htmlFor="bill-adj-reason"
          required
          error={adjErrors.reason}
        >
          <Textarea
            id="bill-adj-reason"
            rows={3}
            maxLength={500}
            value={adjReason}
            onChange={(e) => {
              setAdjReason(e.target.value);
              if (adjErrors.reason) {
                setAdjErrors((prev) => ({ ...prev, reason: undefined }));
              }
            }}
            placeholder={t("billing.adjustments.reasonPlaceholder")}
          />
        </FormField>
      </FormModal>

      {/* Generate Bill Modal */}
      <FormModal
        open={genOpen}
        onClose={() => setGenOpen(false)}
        onSubmit={handleSubmitGenerate}
        title={t("billing.generateDialog.title")}
        size="md"
        submitLabel={t("billing.generateDialog.confirm")}
        submitting={generating}
        testId="generate-bill-modal"
      >
        <FormField label={t("billing.generateDialog.billKind")} required>
          <Select
            value={billKind}
            onChange={(e) => {
              const nextKind = e.target.value as "customer" | "upstream";
              setBillKind(nextKind);
              setGenErrors({});
              void loadGenerateCatalog(nextKind, upstreamScopeType);
            }}
            options={[
              {
                value: "customer",
                label: t("billing.billKinds.customer"),
                text: t("billing.billKinds.customer"),
              },
              {
                value: "upstream",
                label: t("billing.billKinds.upstream"),
                text: t("billing.billKinds.upstream"),
              },
            ]}
            className="w-full"
          />
        </FormField>

        {billKind === "customer" ? (
          <FormField
            label={t("billing.generateDialog.tenant")}
            required
            error={genErrors.tenant_code}
          >
            <Select
              value={genTenantCode}
              options={tenantOptions}
              searchable
              disabled={catalogLoading}
              placeholder={t("billing.generateDialog.tenantPlaceholder")}
              onChange={(e) => {
                setGenTenantCode(e.target.value);
                if (genErrors.tenant_code) {
                  setGenErrors((prev) => ({ ...prev, tenant_code: undefined }));
                }
              }}
              className="w-full"
            />
          </FormField>
        ) : (
          <>
            <FormField
              label={t("billing.generateDialog.upstreamScopeType")}
              required
            >
              <Select
                value={upstreamScopeType}
                onChange={(e) => {
                  const nextType = e.target.value as "channel" | "supplier";
                  setUpstreamScopeType(nextType);
                  setUpstreamScopeId("");
                  void loadGenerateCatalog("upstream", nextType);
                }}
                options={[
                  {
                    value: "channel",
                    label: t("billing.generateDialog.upstreamChannel"),
                    text: t("billing.generateDialog.upstreamChannel"),
                  },
                  {
                    value: "supplier",
                    label: t("billing.generateDialog.upstreamSupplier"),
                    text: t("billing.generateDialog.upstreamSupplier"),
                  },
                ]}
                className="w-full"
              />
            </FormField>

            <FormField
              label={t("billing.generateDialog.upstreamScope")}
              required
              error={genErrors.upstream_scope_id}
            >
              <Select
                value={upstreamScopeId}
                options={
                  upstreamScopeType === "channel"
                    ? channelOptions
                    : supplierOptions
                }
                searchable
                disabled={catalogLoading}
                placeholder={
                  upstreamScopeType === "channel"
                    ? t("billing.generateDialog.upstreamChannelPlaceholder")
                    : t("billing.generateDialog.upstreamSupplierPlaceholder")
                }
                onChange={(e) => {
                  setUpstreamScopeId(e.target.value);
                  if (genErrors.upstream_scope_id) {
                    setGenErrors((prev) => ({
                      ...prev,
                      upstream_scope_id: undefined,
                    }));
                  }
                }}
                className="w-full"
              />
            </FormField>
          </>
        )}

        <FormField
          label={t("billing.generateDialog.periodRange")}
          required
          error={genErrors.period}
        >
          <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
            <Input
              type="date"
              value={periodStart}
              onChange={(e) => {
                setPeriodStart(e.target.value);
                if (genErrors.period) {
                  setGenErrors((prev) => ({ ...prev, period: undefined }));
                }
              }}
              aria-label={t("billing.generateDialog.periodStartPlaceholder")}
              className="font-mono-tech"
            />
            <Input
              type="date"
              value={periodEnd}
              onChange={(e) => {
                setPeriodEnd(e.target.value);
                if (genErrors.period) {
                  setGenErrors((prev) => ({ ...prev, period: undefined }));
                }
              }}
              aria-label={t("billing.generateDialog.periodEndPlaceholder")}
              className="font-mono-tech"
            />
          </div>
        </FormField>
      </FormModal>

      {/* Confirms */}
      <ConfirmDialog
        open={Boolean(statusConfirm)}
        onClose={() => setStatusConfirm(null)}
        onConfirm={() => void handleConfirmStatus()}
        loading={statusUpdating}
        danger={statusConfirm?.status === "overdue"}
        title={t("billing.statusConfirm.title")}
        confirmLabel={t("common.confirm")}
        description={
          statusConfirm
            ? t("billing.statusConfirm.message", {
                status: getStatusLabel(statusConfirm.status),
                name: statusConfirm.row.tenant_name || statusConfirm.row.id,
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
        title={t("billing.deleteConfirm.title")}
        confirmLabel={t("common.actions.delete")}
        description={
          deleteTarget
            ? t("billing.deleteConfirm.one", {
                name: getBillDisplayName(deleteTarget),
              })
            : ""
        }
      />

      <ConfirmDialog
        open={Boolean(batchDeleteIds && batchDeleteIds.length > 0)}
        onClose={() => setBatchDeleteIds(null)}
        onConfirm={() => void handleConfirmBatchDelete()}
        loading={deleting}
        danger
        title={t("billing.deleteConfirm.title")}
        confirmLabel={t("common.actions.delete")}
        description={
          batchDeleteIds
            ? t("billing.deleteConfirm.selected", {
                count: batchDeleteIds.length,
              })
            : ""
        }
      />

      <ConfirmDialog
        open={Boolean(voidTarget)}
        onClose={() => setVoidTarget(null)}
        onConfirm={() => void handleConfirmVoidAdjustment()}
        loading={voiding}
        danger
        title={t("billing.adjustments.void")}
        confirmLabel={t("common.confirm")}
        description={t("billing.adjustments.voidConfirm")}
      />
    </div>
  );
}
