import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router";
import { saveAs } from "file-saver";
import { Plus, RefreshCw, Search, Trash2, Upload } from "lucide-react";
import {
  deleteDraft,
  exportPostman,
  importCurl,
  importOpenApi,
  importPostman,
  listDrafts,
  type ConnectorDraft,
} from "@/api/upstream-connectors";
import { ExportDropdown } from "@/components/common/ExportDropdown";
import {
  Button,
  ConfirmDialog,
  DataTablePage,
  Input,
  StatusBadge,
  type DataTableColumn,
} from "@/components/ui";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import { EMPTY_PLACEHOLDER, formatDateTime } from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { canWriteUpstreamConnector } from "@/utils/ops/roles";
import { logger } from "@/utils/sys/logger";
import {
  showErrorToast,
  showSuccessToast,
  showWarningToast,
} from "@/utils/ui/message-guard";
import {
  WorkbenchImportModal,
  type WorkbenchImportTab,
} from "./WorkbenchImportModal";

export function ConnectorDraftsTab() {
  const { t } = useT();
  const navigate = useNavigate();
  const role = useAuthStore((s) => s.info.role);
  const canWrite = canWriteUpstreamConnector(role);

  const [loading, setLoading] = useState(false);
  const [rows, setRows] = useState<ConnectorDraft[]>([]);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [total, setTotal] = useState(0);
  const [keywordInput, setKeywordInput] = useState("");
  const [appliedKeyword, setAppliedKeyword] = useState("");
  const [selectedIds, setSelectedIds] = useState<string[]>([]);

  const [importOpen, setImportOpen] = useState(false);
  const [importing, setImporting] = useState(false);
  const [importTab, setImportTab] = useState<WorkbenchImportTab>("curl");
  const [importCurlText, setImportCurlText] = useState("");
  const [importJsonText, setImportJsonText] = useState("");

  const [deleteTarget, setDeleteTarget] = useState<ConnectorDraft | null>(null);
  const [batchDeleteIds, setBatchDeleteIds] = useState<string[] | null>(null);
  const [deleting, setDeleting] = useState(false);

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const res = await listDrafts({
        q: appliedKeyword || undefined,
        page,
        page_size: pageSize,
      });
      setRows(res?.drafts ?? []);
      setTotal(res?.pagination?.total ?? 0);
      setSelectedIds([]);
    } catch (error) {
      logger.error("Failed to fetch connector drafts:", error);
    } finally {
      setLoading(false);
    }
  }, [appliedKeyword, page, pageSize]);

  useEffect(() => {
    void loadData();
  }, [loadData]);

  const statusLabel = useCallback(
    (status: string) => {
      if (status === "published" || status === "ready") {
        return t("upstreamConnector.statusPublished");
      }
      if (status === "draft") return t("upstreamConnector.statusDraft");
      return status || EMPTY_PLACEHOLDER;
    },
    [t],
  );

  const handleOpenDraft = (row: ConnectorDraft) => {
    navigate(
      `/operations/upstream-supply/workbench?draftId=${encodeURIComponent(row.id)}`,
    );
  };

  const handleConfirmSingleDelete = async () => {
    if (!deleteTarget || deleting) return;
    setDeleting(true);
    try {
      await deleteDraft(deleteTarget.id);
      showSuccessToast(t("common.feedback.deleteSuccess"));
      setDeleteTarget(null);
      await loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to delete draft:", error);
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
        await deleteDraft(id);
      }
      showSuccessToast(t("common.feedback.deleteSuccess"));
      setBatchDeleteIds(null);
      setSelectedIds([]);
      await loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to batch delete drafts:", error);
      }
    } finally {
      setDeleting(false);
    }
  };

  const handleDoImport = async () => {
    if (importing) return;
    setImporting(true);
    try {
      let draft: ConnectorDraft;
      if (importTab === "curl") {
        draft = await importCurl({ curl: importCurlText });
      } else if (importTab === "postman") {
        draft = await importPostman({ collection: JSON.parse(importJsonText) });
      } else {
        draft = await importOpenApi({ document: JSON.parse(importJsonText) });
      }
      setImportOpen(false);
      showSuccessToast(t("upstreamConnector.imported"));
      navigate(
        `/operations/upstream-supply/workbench?draftId=${encodeURIComponent(draft.id)}`,
      );
    } catch (e) {
      showErrorToast(
        (e as Error)?.message || t("common.feedback.unknownError"),
      );
    } finally {
      setImporting(false);
    }
  };

  const handleExportCommand = async (command: string) => {
    const targetRows =
      command === "selected"
        ? rows.filter((r) => selectedIds.includes(r.id))
        : rows;
    if (targetRows.length === 0) {
      showWarningToast(
        command === "selected"
          ? t("feedback.batch.selectDataForAction", {
              action: t("common.actions.export"),
            })
          : t("feedback.batch.noDataForAction", {
              action: t("common.actions.export"),
            }),
      );
      return;
    }
    try {
      for (const row of targetRows) {
        const col = await exportPostman(row.id);
        const blob = new Blob([JSON.stringify(col, null, 2)], {
          type: "application/json",
        });
        saveAs(blob, `${row.name || "connector"}.postman.json`);
      }
      showSuccessToast(t("common.feedback.exportSuccess"));
    } catch (error) {
      logger.error("Failed to export draft:", error);
    }
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

  const columns = useMemo<DataTableColumn<ConnectorDraft>[]>(
    () => [
      {
        id: "name",
        header: t("common.fields.name"),
        minWidth: 160,
        cell: (row) => (
          <button
            type="button"
            onClick={() => handleOpenDraft(row)}
            className="text-left font-medium text-foreground hover:text-primary transition-colors cursor-pointer truncate max-w-[260px] block"
          >
            {row.name || EMPTY_PLACEHOLDER}
          </button>
        ),
      },
      {
        id: "upstream_url",
        header: t("supplier.fields.accessAddress"),
        minWidth: 200,
        mono: true,
        accessorKey: "upstream_url",
      },
      {
        id: "endpoint_count",
        header: t("upstreamConnector.endpointCount"),
        width: 96,
        align: "right",
        mono: true,
        cell: (row) => (Array.isArray(row.endpoints) ? row.endpoints.length : 0),
      },
      {
        id: "status",
        header: t("common.fields.status"),
        width: 108,
        cell: (row) => (
          <StatusBadge
            status={
              row.status === "published" || row.status === "ready"
                ? "PUBLISHED"
                : "DRAFT"
            }
            label={statusLabel(row.status)}
          />
        ),
      },
      {
        id: "updated_at",
        header: t("common.fields.updatedAt"),
        width: 168,
        mono: true,
        cell: (row) => formatDateTime(row.updated_at),
      },
      {
        id: "operations",
        header: t("common.fields.operations"),
        width: canWrite ? 168 : 108,
        align: "right",
        cell: (row) => (
          <div className="inline-flex items-center justify-end gap-1">
            <Button
              type="button"
              variant="link"
              size="xs"
              onClick={() => handleOpenDraft(row)}
            >
              {canWrite
                ? t("upstreamConnector.continueEdit")
                : t("upstreamConnector.viewDraft")}
            </Button>
            {canWrite && (
              <Button
                type="button"
                variant="link"
                size="xs"
                className="text-danger-fg hover:text-danger-fg hover:bg-danger-fg/10"
                onClick={() => setDeleteTarget(row)}
              >
                {t("common.actions.delete")}
              </Button>
            )}
          </div>
        ),
      },
    ],
    [t, canWrite, statusLabel],
  );

  const handleSearch = () => {
    setPage(1);
    setAppliedKeyword(keywordInput.trim());
  };

  return (
    <>
      <DataTablePage
        columns={columns}
        data={rows}
        getRowId={(row) => row.id}
        loading={loading}
        emptyText={
          appliedKeyword
            ? t("upstreamConnector.draftsNoMatch")
            : t("upstreamConnector.draftsEmpty")
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
        testId="connector-drafts-table"
        filters={
          <div className="flex items-center gap-1.5">
            <Input
              value={keywordInput}
              onChange={(e) => setKeywordInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleSearch();
              }}
              placeholder={t("supplier.list.searchPlaceholder")}
              className="w-56"
              data-testid="draft-search-input"
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
                onCommand={(cmd) => void handleExportCommand(cmd)}
              />
            )}
            {canWrite && (
              <Button
                type="button"
                variant="primary"
                size="md"
                onClick={() => navigate("/operations/upstream-supply/workbench")}
              >
                <Plus className="h-3.5 w-3.5" />
                {t("upstreamConnector.codeFreeAccess")}
              </Button>
            )}
          </>
        }
      />

      <WorkbenchImportModal
        open={importOpen}
        onClose={() => setImportOpen(false)}
        importTab={importTab}
        onImportTabChange={setImportTab}
        importCurlText={importCurlText}
        onImportCurlTextChange={setImportCurlText}
        importJsonText={importJsonText}
        onImportJsonTextChange={setImportJsonText}
        importing={importing}
        onConfirm={handleDoImport}
      />

      <ConfirmDialog
        open={Boolean(deleteTarget)}
        onClose={() => setDeleteTarget(null)}
        onConfirm={() => void handleConfirmSingleDelete()}
        title={t("common.tips")}
        description={
          deleteTarget
            ? t("upstreamConnector.deleteDraftConfirm", {
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
        title={t("common.tips")}
        description={t("upstreamConnector.deleteDraftConfirm", {
          name: `${batchDeleteIds?.length ?? 0}`,
        })}
        confirmLabel={t("common.confirm")}
        cancelLabel={t("common.cancel")}
        loading={deleting}
      />
    </>
  );
}
