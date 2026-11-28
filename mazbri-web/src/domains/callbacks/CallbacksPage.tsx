import { useCallback, useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router";
import {
  Plus,
  RefreshCw,
  RotateCw,
  Trash2,
  Upload,
} from "lucide-react";
import {
  createCallbackSubscription,
  deleteCallbackSubscription,
  fetchCallbackDeliveries,
  fetchCallbackSubscriptions,
  retryCallbackDelivery,
  type CallbackDeliveryLog,
  type CallbackSubscription,
} from "@/api/callback";
import { CsvImportModal } from "@/components/common/CsvImportModal";
import { ExportDropdown } from "@/components/common/ExportDropdown";
import {
  Button,
  ConfirmDialog,
  CopyableText,
  DataTablePage,
  ListQueryBar,
  Modal,
  StatusBadge,
  Tabs,
  type DataTableColumn,
  type TabOption,
} from "@/components/ui";
import {
  callbackDeliveriesCatalog,
  callbackSubscriptionsCatalog,
} from "@/list-catalog/callbacks";
import { PageHeaderActions } from "@/components/layout";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import type { ListFilter } from "@/types/list-query";
import { EMPTY_PLACEHOLDER, formatDateTime } from "@/utils/format";
import { getUserFacingErrorMessage } from "@/utils/http/error";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { encodeFilters } from "@/utils/list-query/serialize";
import { isBizRole } from "@/utils/ops/roles";
import {
  exportResourceCsv,
  runResourceCsvImport,
  type ColumnConfig,
} from "@/utils/resource-csv";
import { logger } from "@/utils/sys/logger";
import { showErrorToast, showSuccessToast } from "@/utils/ui/message-guard";
import { CreateCallbackModal } from "./CreateCallbackModal";

type CallbackTab = "subscriptions" | "deliveries";

interface SubscriptionImportRow extends Record<string, unknown> {
  unique_key: string;
  tenant_code: string;
  product_code: string;
  callback_url: string;
  secret_key: string;
}

interface SubscriptionExportRow extends Record<string, unknown> {
  tenant_code: string;
  tenant_id: string;
  product_code: string;
  callback_url: string;
  status: string;
}

function resolveTab(raw: string | null): CallbackTab {
  return raw === "deliveries" ? "deliveries" : "subscriptions";
}

function deliveryStatusKey(status: string): string {
  if (status === "success") return "SUCCESS";
  if (status === "failed" || status === "abandoned") return "FAILED";
  if (status === "retrying") return "WARNING";
  return "PENDING";
}

export function CallbacksPage() {
  const { t } = useT();
  const [searchParams, setSearchParams] = useSearchParams();
  const role = useAuthStore((s) => s.info.role);
  const canWrite = isBizRole(role);

  const activeTab = resolveTab(searchParams.get("tab"));
  const [loadedTabs, setLoadedTabs] = useState<Set<CallbackTab>>(
    () => new Set([activeTab]),
  );

  // Subscriptions state
  const [subLoading, setSubLoading] = useState(false);
  const [subscriptions, setSubscriptions] = useState<CallbackSubscription[]>(
    [],
  );
  const [subPage, setSubPage] = useState(1);
  const [subPageSize, setSubPageSize] = useState(20);
  const [subTotal, setSubTotal] = useState(0);
  const [subQInput, setSubQInput] = useState("");
  const [subAppliedQ, setSubAppliedQ] = useState("");
  const [subFilters, setSubFilters] = useState<ListFilter[]>([]);
  const [selectedSubIds, setSelectedSubIds] = useState<string[]>([]);

  const [createOpen, setCreateOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] =
    useState<CallbackSubscription | null>(null);
  const [batchDeleteIds, setBatchDeleteIds] = useState<string[] | null>(null);
  const [deleting, setDeleting] = useState(false);

  // Deliveries state
  const [delLoading, setDelLoading] = useState(false);
  const [deliveries, setDeliveries] = useState<CallbackDeliveryLog[]>([]);
  const [delPage, setDelPage] = useState(1);
  const [delPageSize, setDelPageSize] = useState(20);
  const [delTotal, setDelTotal] = useState(0);
  const [delQInput, setDelQInput] = useState("");
  const [delAppliedQ, setDelAppliedQ] = useState("");
  const [delFilters, setDelFilters] = useState<ListFilter[]>([]);
  const [detailDelivery, setDetailDelivery] =
    useState<CallbackDeliveryLog | null>(null);
  const [retryingId, setRetryingId] = useState<string | null>(null);

  useEffect(() => {
    setLoadedTabs((prev) => {
      if (prev.has(activeTab)) return prev;
      const next = new Set(prev);
      next.add(activeTab);
      return next;
    });
  }, [activeTab]);

  const handleTabChange = (next: CallbackTab) => {
    const nextParams = new URLSearchParams(searchParams);
    nextParams.set("tab", next);
    setSearchParams(nextParams, { replace: true });
  };

  const loadSubscriptions = useCallback(async () => {
    setSubLoading(true);
    try {
      const res = await fetchCallbackSubscriptions({
        q: subAppliedQ || undefined,
        filters: encodeFilters(subFilters).filters,
        page: subPage,
        page_size: subPageSize,
      });
      setSubscriptions(res.data ?? []);
      setSubTotal(res.total ?? 0);
      setSelectedSubIds([]);
    } catch (error) {
      showErrorToast(
        getUserFacingErrorMessage(error, t("callbacks.messages.fetchListFailed")),
      );
    } finally {
      setSubLoading(false);
    }
  }, [subAppliedQ, subFilters, subPage, subPageSize, t]);

  const loadDeliveries = useCallback(async () => {
    setDelLoading(true);
    try {
      const res = await fetchCallbackDeliveries({
        q: delAppliedQ || undefined,
        filters: encodeFilters(delFilters).filters,
        page: delPage,
        page_size: delPageSize,
      });
      setDeliveries(res.data ?? []);
      setDelTotal(res.total ?? 0);
    } catch (error) {
      showErrorToast(
        getUserFacingErrorMessage(
          error,
          t("callbacks.messages.fetchDeliveriesFailed"),
        ),
      );
    } finally {
      setDelLoading(false);
    }
  }, [delAppliedQ, delFilters, delPage, delPageSize, t]);

  const handleSubSearch = useCallback(() => {
    setSubPage(1);
    setSubAppliedQ(subQInput.trim());
  }, [subQInput]);

  const handleSubFiltersChange = useCallback((next: ListFilter[]) => {
    setSubPage(1);
    setSubFilters(next);
  }, []);

  const handleDelSearch = useCallback(() => {
    setDelPage(1);
    setDelAppliedQ(delQInput.trim());
  }, [delQInput]);

  const handleDelFiltersChange = useCallback((next: ListFilter[]) => {
    setDelPage(1);
    setDelFilters(next);
  }, []);

  useEffect(() => {
    if (loadedTabs.has("subscriptions")) {
      void loadSubscriptions();
    }
  }, [loadedTabs, loadSubscriptions]);

  useEffect(() => {
    if (loadedTabs.has("deliveries")) {
      void loadDeliveries();
    }
  }, [loadedTabs, loadDeliveries]);

  const deliveryStatusLabel = useCallback(
    (status: string) => {
      if (status === "success") return t("callbacks.status.success");
      if (status === "failed") return t("callbacks.status.failed");
      if (status === "abandoned") return t("callbacks.status.abandoned");
      if (status === "retrying") return t("callbacks.status.retrying");
      if (status === "pending") return t("common.status.pending");
      return status || EMPTY_PLACEHOLDER;
    },
    [t],
  );

  const handleConfirmSingleDelete = async () => {
    if (!deleteTarget || deleting) return;
    setDeleting(true);
    try {
      await deleteCallbackSubscription(deleteTarget.id);
      showSuccessToast(t("common.feedback.deleteSuccess"));
      setDeleteTarget(null);
      await loadSubscriptions();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to delete callback subscription:", error);
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
        await deleteCallbackSubscription(id);
      }
      showSuccessToast(t("common.feedback.deleteSuccess"));
      setBatchDeleteIds(null);
      setSelectedSubIds([]);
      await loadSubscriptions();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to batch delete callback subscriptions:", error);
      }
    } finally {
      setDeleting(false);
    }
  };

  const handleRetryDelivery = async (row: CallbackDeliveryLog) => {
    if (retryingId) return;
    setRetryingId(row.id);
    try {
      await retryCallbackDelivery(row.id);
      showSuccessToast(t("callbacks.messages.retrySuccess"));
      await loadDeliveries();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        showErrorToast(
          getUserFacingErrorMessage(error, t("callbacks.messages.retryFailed")),
        );
      }
    } finally {
      setRetryingId(null);
    }
  };

  // CSV import / export for subscriptions
  const importColumns = useMemo<ColumnConfig[]>(
    () => [
      {
        key: "tenant_code",
        label: t("callbacks.columns.tenantCode"),
        required: true,
      },
      {
        key: "product_code",
        label: t("callbacks.columns.productCode"),
        required: true,
      },
      {
        key: "callback_url",
        label: t("callbacks.columns.callbackUrl"),
        required: true,
      },
      {
        key: "secret_key",
        label: t("callbacks.fields.secret"),
        required: true,
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

  const importExistingData = useMemo(
    () =>
      subscriptions.map((row) => ({
        unique_key: `${row.tenant_code}/${row.product_code}`,
      })),
    [subscriptions],
  );

  const validateImportRow = (row: Record<string, string>) => {
    if (!row.tenant_code?.trim()) {
      return {
        valid: false,
        reason: t("callbacks.validation.tenantCodeRequired"),
      };
    }
    if (!row.product_code?.trim()) {
      return {
        valid: false,
        reason: t("callbacks.validation.productCodeRequired"),
      };
    }
    if (!row.callback_url?.trim()) {
      return {
        valid: false,
        reason: t("callbacks.validation.callbackUrlRequired"),
      };
    }
    if (!row.secret_key?.trim()) {
      return {
        valid: false,
        reason: t("callbacks.validation.secretKeyRequired"),
      };
    }
    return { valid: true };
  };

  const transformImportRow = (
    row: Record<string, string>,
  ): SubscriptionImportRow => ({
    unique_key: `${row.tenant_code.trim()}/${row.product_code.trim()}`,
    tenant_code: row.tenant_code.trim(),
    product_code: row.product_code.trim(),
    callback_url: row.callback_url.trim(),
    secret_key: row.secret_key.trim(),
  });

  const handleImport = async (items: Record<string, unknown>[]) => {
    await runResourceCsvImport<SubscriptionImportRow>(
      items,
      async (row) => {
        await createCallbackSubscription(
          {
            tenant_code: row.tenant_code,
            product_code: row.product_code,
            callback_url: row.callback_url,
            secret_key: row.secret_key,
          },
          { showSuccessMessage: false },
        );
      },
      {
        onError: (error) => {
          logger.error("Failed to import callback subscription:", error);
        },
      },
    );
  };

  const exportItems = useMemo(
    () => [
      {
        command: "page",
        label: t("common.actions.exportPage"),
        disabled: subscriptions.length === 0,
      },
      {
        command: "selected",
        label: t("common.actions.exportSelected", {
          count: selectedSubIds.length,
        }),
        disabled: selectedSubIds.length === 0,
      },
    ],
    [t, subscriptions.length, selectedSubIds.length],
  );

  const handleExportCommand = (command: string) => {
    const rows =
      command === "selected"
        ? subscriptions.filter((r) => selectedSubIds.includes(r.id))
        : subscriptions;
    const csvRows: SubscriptionExportRow[] = rows.map((row) => ({
      tenant_code: row.tenant_code,
      tenant_id: row.tenant_id,
      product_code: row.product_code,
      callback_url: row.callback_url,
      status: row.is_active
        ? t("common.status.enabled")
        : t("common.status.disabled"),
    }));
    exportResourceCsv(
      csvRows,
      [
        { key: "tenant_code", label: t("callbacks.columns.tenantCode") },
        { key: "tenant_id", label: t("callbacks.columns.tenantAccount") },
        { key: "product_code", label: t("callbacks.columns.productCode") },
        { key: "callback_url", label: t("callbacks.columns.callbackUrl") },
        { key: "status", label: t("common.fields.status") },
      ],
      `callback-subscriptions-${command}-${Date.now().toString(36)}.csv`,
    );
  };

  const subscriptionColumns = useMemo<DataTableColumn<CallbackSubscription>[]>(
    () => {
      const cols: DataTableColumn<CallbackSubscription>[] = [
        {
          id: "tenant_code",
          header: t("callbacks.columns.tenantCode"),
          width: 136,
          cell: (row) => <CopyableText value={row.tenant_code} />,
        },
        {
          id: "tenant_id",
          header: t("callbacks.columns.tenantAccount"),
          width: 168,
          cell: (row) => (
            <CopyableText
              value={row.tenant_id}
              truncate
              maxWidthClass="max-w-[140px]"
            />
          ),
        },
        {
          id: "product_code",
          header: t("callbacks.columns.productCode"),
          width: 152,
          cell: (row) => <CopyableText value={row.product_code} />,
        },
        {
          id: "callback_url",
          header: t("callbacks.columns.callbackUrl"),
          minWidth: 200,
          cell: (row) => (
            <CopyableText
              value={row.callback_url}
              truncate
              maxWidthClass="max-w-[300px]"
            />
          ),
        },
        {
          id: "is_active",
          header: t("common.fields.status"),
          width: 104,
          cell: (row) => (
            <StatusBadge
              status={row.is_active ? "ENABLED" : "DISABLED"}
              label={
                row.is_active
                  ? t("common.status.enabled")
                  : t("common.status.disabled")
              }
            />
          ),
        },
      ];
      if (canWrite) {
        cols.push({
          id: "operations",
          header: t("common.fields.operations"),
          width: 96,
          align: "right",
          cell: (row) => (
            <Button
              type="button"
              variant="link"
              size="xs"
              className="text-muted-foreground hover:text-danger-fg hover:bg-danger-fg/10"
              onClick={() => setDeleteTarget(row)}
            >
              {t("common.actions.delete")}
            </Button>
          ),
        });
      }
      return cols;
    },
    [t, canWrite],
  );

  const deliveryColumns = useMemo<DataTableColumn<CallbackDeliveryLog>[]>(
    () => [
      {
        id: "event_id",
        header: t("callbacks.columns.eventId"),
        width: 180,
        cell: (row) => (
          <CopyableText
            value={row.event_id}
            truncate
            maxWidthClass="max-w-[150px]"
          />
        ),
      },
      {
        id: "callback_url",
        header: t("callbacks.columns.callbackUrl"),
        minWidth: 180,
        cell: (row) => (
          <CopyableText
            value={row.callback_url}
            truncate
            maxWidthClass="max-w-[240px]"
          />
        ),
      },
      {
        id: "status",
        header: t("common.fields.status"),
        width: 108,
        cell: (row) => (
          <StatusBadge
            status={deliveryStatusKey(row.status)}
            label={deliveryStatusLabel(row.status)}
          />
        ),
      },
      {
        id: "retry_count",
        header: t("callbacks.columns.retryCount"),
        width: 96,
        align: "right",
        mono: true,
        accessorKey: "retry_count",
      },
      {
        id: "last_response_code",
        header: t("callbacks.columns.responseCode"),
        width: 104,
        align: "right",
        mono: true,
        cell: (row) =>
          row.last_response_code != null
            ? String(row.last_response_code)
            : EMPTY_PLACEHOLDER,
      },
      {
        id: "last_error",
        header: t("callbacks.columns.failureReason"),
        minWidth: 140,
        cell: (row) =>
          row.last_error ? (
            <span
              className="block max-w-[220px] truncate text-danger-fg"
              title={row.last_error}
            >
              {row.last_error}
            </span>
          ) : (
            <span className="text-muted-foreground">{EMPTY_PLACEHOLDER}</span>
          ),
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
        width: canWrite ? 140 : 84,
        align: "right",
        cell: (row) => (
          <div className="inline-flex items-center justify-end gap-1">
            <Button
              type="button"
              variant="link"
              size="xs"
              onClick={() => setDetailDelivery(row)}
            >
              {t("common.actions.details")}
            </Button>
            {canWrite && (
              <Button
                type="button"
                variant="link"
                size="xs"
                disabled={row.status === "success" || retryingId === row.id}
                onClick={() => void handleRetryDelivery(row)}
              >
                <RotateCw className="h-3 w-3" />
                {t("callbacks.actions.retry")}
              </Button>
            )}
          </div>
        ),
      },
    ],
    [t, canWrite, deliveryStatusLabel, retryingId],
  );

  const tabItems: TabOption<CallbackTab>[] = [
    {
      key: "subscriptions",
      label: t("callbacks.tabs.subscriptions"),
      testId: "tab-subscriptions",
    },
    {
      key: "deliveries",
      label: t("callbacks.tabs.deliveries"),
      testId: "tab-deliveries",
    },
  ];

  return (
    <div
      className="flex flex-col flex-1 min-h-0 gap-4 min-w-0"
      data-testid="callbacks-page"
    >
      <PageHeaderActions>
        <Tabs
          variant="inline"
          items={tabItems}
          value={activeTab}
          onChange={handleTabChange}
        />
      </PageHeaderActions>

      {loadedTabs.has("subscriptions") && (
        <div
          className={
            activeTab === "subscriptions"
              ? "flex flex-col flex-1 min-h-0 min-w-0"
              : "hidden"
          }
        >
          <DataTablePage
            columns={subscriptionColumns}
            data={subscriptions}
            getRowId={(row) => row.id}
            loading={subLoading}
            selectable={canWrite}
            selectedKeys={selectedSubIds}
            onSelectionChange={setSelectedSubIds}
            page={subPage}
            pageSize={subPageSize}
            total={subTotal}
            pageSizeOptions={[10, 20, 50]}
            onPageChange={setSubPage}
            onPageSizeChange={(next) => {
              setSubPageSize(next);
              setSubPage(1);
            }}
            testId="callback-subscriptions-table"
            filters={
              <ListQueryBar
                q={subQInput}
                onQChange={setSubQInput}
                filters={subFilters}
                onFiltersChange={handleSubFiltersChange}
                catalog={callbackSubscriptionsCatalog}
                onSearch={handleSubSearch}
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
                      data-testid="callback-batch-delete-btn"
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
                  onClick={() => void loadSubscriptions()}
                  disabled={subLoading}
                >
                  <RefreshCw
                    className={
                      subLoading ? "h-3.5 w-3.5 animate-spin" : "h-3.5 w-3.5"
                    }
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
                  disabled={subscriptions.length === 0}
                  onCommand={handleExportCommand}
                />
                {canWrite && (
                  <Button
                    type="button"
                    variant="primary"
                    size="md"
                    onClick={() => setCreateOpen(true)}
                    data-testid="callback-create-btn"
                  >
                    <Plus className="h-3.5 w-3.5" />
                    {t("callbacks.actions.addSubscription")}
                  </Button>
                )}
              </>
            }
          />
        </div>
      )}

      {loadedTabs.has("deliveries") && (
        <div
          className={
            activeTab === "deliveries"
              ? "flex flex-col flex-1 min-h-0 min-w-0"
              : "hidden"
          }
        >
          <DataTablePage
            columns={deliveryColumns}
            data={deliveries}
            getRowId={(row) => row.id}
            loading={delLoading}
            page={delPage}
            pageSize={delPageSize}
            total={delTotal}
            pageSizeOptions={[10, 20, 50]}
            onPageChange={setDelPage}
            onPageSizeChange={(next) => {
              setDelPageSize(next);
              setDelPage(1);
            }}
            testId="callback-deliveries-table"
            filters={
              <ListQueryBar
                q={delQInput}
                onQChange={setDelQInput}
                filters={delFilters}
                onFiltersChange={handleDelFiltersChange}
                catalog={callbackDeliveriesCatalog}
                onSearch={handleDelSearch}
              />
            }
            actions={
              <Button
                type="button"
                variant="ghost"
                size="md"
                onClick={() => void loadDeliveries()}
                disabled={delLoading}
              >
                <RefreshCw
                  className={
                    delLoading ? "h-3.5 w-3.5 animate-spin" : "h-3.5 w-3.5"
                  }
                />
                {t("common.actions.refresh")}
              </Button>
            }
          />
        </div>
      )}

      <CreateCallbackModal
        open={createOpen}
        onClose={() => setCreateOpen(false)}
        onSuccess={loadSubscriptions}
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
        onSuccess={() => void loadSubscriptions()}
      />

      <ConfirmDialog
        open={Boolean(deleteTarget)}
        onClose={() => setDeleteTarget(null)}
        onConfirm={() => void handleConfirmSingleDelete()}
        title={t("common.actions.delete")}
        description={t("callbacks.messages.deleteConfirm")}
        confirmLabel={t("common.confirm")}
        cancelLabel={t("common.cancel")}
        loading={deleting}
      />

      <ConfirmDialog
        open={Boolean(batchDeleteIds)}
        onClose={() => setBatchDeleteIds(null)}
        onConfirm={() => void handleConfirmBatchDelete()}
        title={t("common.actions.delete")}
        description={t("callbacks.messages.deleteConfirm")}
        confirmLabel={t("common.confirm")}
        cancelLabel={t("common.cancel")}
        loading={deleting}
      />

      <Modal
        open={Boolean(detailDelivery)}
        onClose={() => setDetailDelivery(null)}
        title={t("callbacks.tabs.deliveries")}
        subtitle={detailDelivery?.event_id}
        size="md"
        footer={
          <>
            {canWrite &&
              detailDelivery &&
              detailDelivery.status !== "success" && (
                <Button
                  type="button"
                  variant="primary"
                  loading={retryingId === detailDelivery.id}
                  onClick={async () => {
                    await handleRetryDelivery(detailDelivery);
                    setDetailDelivery(null);
                  }}
                >
                  <RotateCw className="h-3.5 w-3.5" />
                  {t("callbacks.actions.retry")}
                </Button>
              )}
            <Button
              type="button"
              variant="outline"
              onClick={() => setDetailDelivery(null)}
            >
              {t("common.actions.close")}
            </Button>
          </>
        }
      >
        {detailDelivery && (
          <dl className="border border-border rounded-lg overflow-hidden divide-y divide-border">
            <div className="grid grid-cols-[128px_1fr]">
              <dt className="bg-card px-3 py-2 text-muted-foreground border-r border-border">
                {t("callbacks.columns.eventId")}
              </dt>
              <dd className="px-3 py-2 min-w-0">
                <CopyableText value={detailDelivery.event_id} />
              </dd>
            </div>
            <div className="grid grid-cols-[128px_1fr]">
              <dt className="bg-card px-3 py-2 text-muted-foreground border-r border-border">
                {t("callbacks.columns.callbackUrl")}
              </dt>
              <dd className="px-3 py-2 min-w-0">
                <CopyableText value={detailDelivery.callback_url} />
              </dd>
            </div>
            <div className="grid grid-cols-[128px_1fr]">
              <dt className="bg-card px-3 py-2 text-muted-foreground border-r border-border">
                {t("common.fields.status")}
              </dt>
              <dd className="px-3 py-2">
                <StatusBadge
                  status={deliveryStatusKey(detailDelivery.status)}
                  label={deliveryStatusLabel(detailDelivery.status)}
                />
              </dd>
            </div>
            <div className="grid grid-cols-[128px_1fr]">
              <dt className="bg-card px-3 py-2 text-muted-foreground border-r border-border">
                {t("callbacks.columns.retryCount")}
              </dt>
              <dd className="px-3 py-2 font-mono-tech tabular-nums">
                {detailDelivery.retry_count}
              </dd>
            </div>
            <div className="grid grid-cols-[128px_1fr]">
              <dt className="bg-card px-3 py-2 text-muted-foreground border-r border-border">
                {t("callbacks.columns.responseCode")}
              </dt>
              <dd className="px-3 py-2 font-mono-tech tabular-nums">
                {detailDelivery.last_response_code ?? EMPTY_PLACEHOLDER}
              </dd>
            </div>
            <div className="grid grid-cols-[128px_1fr]">
              <dt className="bg-card px-3 py-2 text-muted-foreground border-r border-border">
                {t("callbacks.columns.failureReason")}
              </dt>
              <dd className="px-3 py-2 break-all text-danger-fg">
                {detailDelivery.last_error || EMPTY_PLACEHOLDER}
              </dd>
            </div>
            <div className="grid grid-cols-[128px_1fr]">
              <dt className="bg-card px-3 py-2 text-muted-foreground border-r border-border">
                {t("common.fields.createdAt")}
              </dt>
              <dd className="px-3 py-2 font-mono-tech">
                {formatDateTime(detailDelivery.created_at)}
              </dd>
            </div>
            <div className="grid grid-cols-[128px_1fr]">
              <dt className="bg-card px-3 py-2 text-muted-foreground border-r border-border">
                {t("common.fields.updatedAt")}
              </dt>
              <dd className="px-3 py-2 font-mono-tech">
                {formatDateTime(detailDelivery.updated_at)}
              </dd>
            </div>
          </dl>
        )}
      </Modal>
    </div>
  );
}
