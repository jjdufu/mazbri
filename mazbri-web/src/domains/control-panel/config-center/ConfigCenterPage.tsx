import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Download,
  Plus,
  RefreshCw,
  RotateCcw,
  Search,
  Upload,
} from "lucide-react";
import {
  deleteConfig,
  fetchConfig,
  fetchConfigHistory,
  fetchConfigList,
  fetchServiceList,
} from "@/api/config-center";
import { fetchCanonicalServiceNames } from "@/api/service-names";
import {
  AuthorizationTooltipButton,
  Button,
  ConfirmDialog,
  DataTablePage,
  Input,
  type DataTableColumn,
} from "@/components/ui";
import { useT } from "@/locales";
import type { ConfigEntry, ConfigSummary } from "@/types/api/config-center";
import { EMPTY_PLACEHOLDER, formatDateTime } from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { logger } from "@/utils/sys/logger";
import {
  showSuccessToast,
  showWarningToast,
} from "@/utils/ui/message-guard";
import { useLicenseGate } from "../shared/useLicenseGate";
import { ConfigEditDrawer } from "./ConfigEditDrawer";
import {
  ConfigHistoryDrawer,
  type ConfigHistoryInitialData,
} from "./ConfigHistoryDrawer";
import { ExportConfigModal } from "./ExportConfigModal";
import { ImportConfigModal } from "./ImportConfigModal";
import {
  FALLBACK_CORE_CONFIG_NAMES,
  isCoreProtectedConfigName,
} from "./helpers";

export function ConfigCenterPage() {
  const { t } = useT();
  const {
    authorizationDisabled,
    authorizationDisabledReason,
    loadLicenseStatus,
    ensureLicenseActive,
  } = useLicenseGate();

  const [rows, setRows] = useState<ConfigSummary[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [loading, setLoading] = useState(false);

  const [keywordInput, setKeywordInput] = useState("");
  const [appliedKeyword, setAppliedKeyword] = useState("");

  const [allExistingNames, setAllExistingNames] = useState<string[]>([]);
  const [coreProtectedSet, setCoreProtectedSet] = useState<Set<string>>(
    () => new Set(FALLBACK_CORE_CONFIG_NAMES),
  );

  // U1 MFA-safe pre-fetch states: drawer opens ONLY after protected fetch succeeds
  const [pendingActionKey, setPendingActionKey] = useState<string | null>(null);
  const [editDrawerOpen, setEditDrawerOpen] = useState(false);
  const [editingEntry, setEditingEntry] = useState<ConfigEntry | null>(null);

  const [historyDrawerOpen, setHistoryDrawerOpen] = useState(false);
  const [historyInitialData, setHistoryInitialData] =
    useState<ConfigHistoryInitialData | null>(null);

  const [importOpen, setImportOpen] = useState(false);
  const [exportOpen, setExportOpen] = useState(false);

  const [deleteTarget, setDeleteTarget] = useState<ConfigSummary | null>(null);
  const [deleting, setDeleting] = useState(false);

  const loadExistingNames = useCallback(async () => {
    try {
      const res = await fetchServiceList();
      setAllExistingNames(res.services ?? []);
    } catch (error) {
      logger.error("Failed to load existing config service names:", error);
    }
  }, []);

  const loadCoreProtected = useCallback(async () => {
    try {
      const data = await fetchCanonicalServiceNames();
      setCoreProtectedSet(
        new Set([
          ...FALLBACK_CORE_CONFIG_NAMES,
          data.global,
          ...(data.services ?? []),
        ]),
      );
    } catch (error) {
      logger.error("Failed to load canonical service names:", error);
    }
  }, []);

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const res = await fetchConfigList({
        page,
        page_size: pageSize,
        q: appliedKeyword || undefined,
      });
      setRows(res.configs ?? []);
      setTotal(res.total ?? 0);
    } catch (error) {
      logger.error("Failed to load config list:", error);
    } finally {
      setLoading(false);
    }
  }, [page, pageSize, appliedKeyword]);

  useEffect(() => {
    void loadLicenseStatus();
    void loadCoreProtected();
    void loadExistingNames();
  }, [loadLicenseStatus, loadCoreProtected, loadExistingNames]);

  useEffect(() => {
    void loadData();
  }, [loadData]);

  const existingServiceNames = useMemo(
    () =>
      Array.from(
        new Set([...allExistingNames, ...rows.map((r) => r.service_name)]),
      ),
    [allExistingNames, rows],
  );

  const handleSearchSubmit = () => {
    setPage(1);
    setAppliedKeyword(keywordInput.trim());
  };

  const handleResetFilters = () => {
    setKeywordInput("");
    setAppliedKeyword("");
    setPage(1);
  };

  const handleCreate = () => {
    if (!ensureLicenseActive()) return;
    setEditingEntry(null);
    setEditDrawerOpen(true);
  };

  /**
   * U1 MFA Step-Up Guard:
   * Fetch protected config detail FIRST. Open drawer ONLY on success.
   * If user cancels MFA step-up dialog, abort silently with zero empty drawer & zero error toast.
   */
  const handleEdit = useCallback(
    async (row: ConfigSummary) => {
      if (authorizationDisabled) {
        showWarningToast(authorizationDisabledReason);
        return;
      }
      const key = `edit:${row.service_name}`;
      if (pendingActionKey) return;
      setPendingActionKey(key);
      try {
        const detail = await fetchConfig(row.service_name);
        setEditingEntry(detail);
        setEditDrawerOpen(true);
      } catch (error) {
        if (!isMfaStepUpCancelled(error)) {
          logger.error("Failed to fetch config detail:", error);
        }
      } finally {
        setPendingActionKey(null);
      }
    },
    [authorizationDisabled, authorizationDisabledReason, pendingActionKey],
  );

  /**
   * U1 MFA Step-Up Guard for version history:
   * Fetch active config + history page FIRST. Open drawer ONLY on success.
   */
  const handleHistory = useCallback(
    async (row: ConfigSummary) => {
      const key = `history:${row.service_name}`;
      if (pendingActionKey) return;
      setPendingActionKey(key);
      try {
        const [detail, historyRes] = await Promise.all([
          fetchConfig(row.service_name),
          fetchConfigHistory({
            service_name: row.service_name,
            page: 1,
            page_size: 10,
          }),
        ]);
        setHistoryInitialData({
          config: row,
          activeContent: detail.content,
          history: historyRes.history ?? [],
          total: historyRes.total ?? 0,
        });
        setHistoryDrawerOpen(true);
      } catch (error) {
        if (!isMfaStepUpCancelled(error)) {
          logger.error("Failed to fetch config history:", error);
        }
      } finally {
        setPendingActionKey(null);
      }
    },
    [pendingActionKey],
  );

  const handleConfirmDelete = async () => {
    if (!deleteTarget || deleting) return;
    if (isCoreProtectedConfigName(deleteTarget.service_name, coreProtectedSet)) {
      setDeleteTarget(null);
      return;
    }
    setDeleting(true);
    try {
      await deleteConfig(deleteTarget.service_name);
      showSuccessToast(t("common.feedback.deleteSuccess"));
      setDeleteTarget(null);
      await Promise.all([loadExistingNames(), loadData()]);
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to delete config:", error);
      }
    } finally {
      setDeleting(false);
    }
  };

  const columns = useMemo<DataTableColumn<ConfigSummary>[]>(
    () => [
      {
        id: "service_name",
        header: t("configCenter.columns.serviceName"),
        minWidth: 220,
        cell: (row) => (
          <span className="font-medium font-mono-tech text-foreground">
            {row.service_name}
          </span>
        ),
      },
      {
        id: "version",
        header: t("configCenter.columns.version"),
        width: 96,
        mono: true,
        cell: (row) => (
          <span className="tabular-nums">v{row.version ?? 1}</span>
        ),
      },
      {
        id: "updated_by",
        header: t("configCenter.columns.updatedBy"),
        minWidth: 140,
        cell: (row) => row.updated_by || EMPTY_PLACEHOLDER,
      },
      {
        id: "updated_at",
        header: t("configCenter.columns.updatedAt"),
        minWidth: 180,
        mono: true,
        cell: (row) => formatDateTime(row.updated_at),
      },
      {
        id: "operation",
        header: t("configCenter.columns.operation"),
        width: 240,
        align: "right",
        cell: (row) => {
          const isProtected = isCoreProtectedConfigName(
            row.service_name,
            coreProtectedSet,
          );
          const editLoading = pendingActionKey === `edit:${row.service_name}`;
          const historyLoading =
            pendingActionKey === `history:${row.service_name}`;

          return (
            <div className="inline-flex items-center justify-end gap-1">
              <AuthorizationTooltipButton
                type="button"
                variant="link"
                size="xs"
                loading={editLoading}
                disabled={
                  authorizationDisabled ||
                  (pendingActionKey !== null && !editLoading)
                }
                disabledReason={
                  authorizationDisabled
                    ? authorizationDisabledReason
                    : undefined
                }
                onClick={() => void handleEdit(row)}
              >
                {t("configCenter.actions.edit")}
              </AuthorizationTooltipButton>

              <Button
                type="button"
                variant="link"
                size="xs"
                loading={historyLoading}
                disabled={pendingActionKey !== null && !historyLoading}
                onClick={() => void handleHistory(row)}
              >
                {t("configCenter.actions.history")}
              </Button>

              <AuthorizationTooltipButton
                type="button"
                variant="link"
                size="xs"
                disabled={authorizationDisabled || isProtected}
                disabledReason={
                  isProtected
                    ? t("configCenter.messages.coreProtectedDeleteTip")
                    : authorizationDisabledReason
                }
                className="text-danger-fg hover:text-danger-fg hover:bg-danger-fg/10"
                onClick={() => setDeleteTarget(row)}
              >
                {t("common.actions.delete")}
              </AuthorizationTooltipButton>
            </div>
          );
        },
      },
    ],
    [
      t,
      coreProtectedSet,
      pendingActionKey,
      authorizationDisabled,
      authorizationDisabledReason,
      handleEdit,
      handleHistory,
    ],
  );

  return (
    <>
      <DataTablePage<ConfigSummary>
        testId="config-center-page"
        columns={columns}
        data={rows}
        getRowId={(row) => row.id || row.service_name}
        loading={loading}
        emptyText={
          appliedKeyword
            ? t("common.feedback.noMatchingData")
            : t("common.feedback.noData")
        }
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
          <>
            <div className="relative w-56">
              <Search className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
              <Input
                value={keywordInput}
                onChange={(e) => setKeywordInput(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleSearchSubmit();
                }}
                placeholder={t("common.search")}
                aria-label={t("common.search")}
                className="pl-8"
                data-testid="config-center-search-input"
              />
            </div>

            <Button
              type="button"
              variant="outline"
              size="md"
              onClick={handleSearchSubmit}
            >
              {t("common.search")}
            </Button>

            {appliedKeyword && (
              <Button
                type="button"
                variant="ghost"
                size="md"
                onClick={handleResetFilters}
              >
                <RotateCcw className="h-3.5 w-3.5" />
                {t("common.reset")}
              </Button>
            )}
          </>
        }
        actions={
          <>
            <AuthorizationTooltipButton
              type="button"
              variant="outline"
              size="md"
              disabled={authorizationDisabled}
              disabledReason={authorizationDisabledReason}
              onClick={() => setImportOpen(true)}
            >
              <Upload className="h-3.5 w-3.5" />
              {t("configCenter.actions.import")}
            </AuthorizationTooltipButton>

            <AuthorizationTooltipButton
              type="button"
              variant="outline"
              size="md"
              disabled={authorizationDisabled || rows.length === 0}
              disabledReason={
                authorizationDisabled ? authorizationDisabledReason : undefined
              }
              onClick={() => setExportOpen(true)}
            >
              <Download className="h-3.5 w-3.5" />
              {t("configCenter.actions.export")}
            </AuthorizationTooltipButton>

            <Button
              type="button"
              variant="outline"
              size="md"
              onClick={() => void loadData()}
            >
              <RefreshCw className="h-3.5 w-3.5" />
              {t("common.actions.refresh")}
            </Button>

            <AuthorizationTooltipButton
              type="button"
              variant="primary"
              size="md"
              disabled={authorizationDisabled}
              disabledReason={authorizationDisabledReason}
              onClick={handleCreate}
              data-testid="config-center-create-btn"
            >
              <Plus className="h-3.5 w-3.5" />
              {t("configCenter.actions.create")}
            </AuthorizationTooltipButton>
          </>
        }
      />

      <ConfigEditDrawer
        open={editDrawerOpen}
        config={editingEntry}
        existingNames={existingServiceNames}
        authorizationDisabled={authorizationDisabled}
        authorizationDisabledReason={authorizationDisabledReason}
        onClose={() => setEditDrawerOpen(false)}
        onSuccess={() => {
          setKeywordInput("");
          setAppliedKeyword("");
          setPage(1);
          void Promise.all([loadExistingNames(), loadData()]);
        }}
      />

      <ConfigHistoryDrawer
        open={historyDrawerOpen}
        initialData={historyInitialData}
        authorizationDisabled={authorizationDisabled}
        authorizationDisabledReason={authorizationDisabledReason}
        onClose={() => setHistoryDrawerOpen(false)}
        onRollbackSuccess={() => {
          void Promise.all([loadExistingNames(), loadData()]);
        }}
      />

      <ImportConfigModal
        open={importOpen}
        authorizationDisabled={authorizationDisabled}
        authorizationDisabledReason={authorizationDisabledReason}
        onClose={() => setImportOpen(false)}
        onSuccess={() => {
          void Promise.all([loadExistingNames(), loadData()]);
        }}
      />

      <ExportConfigModal
        open={exportOpen}
        authorizationDisabled={authorizationDisabled}
        authorizationDisabledReason={authorizationDisabledReason}
        onClose={() => setExportOpen(false)}
        onSuccess={() => {}}
      />

      <ConfirmDialog
        open={deleteTarget !== null}
        onClose={() => setDeleteTarget(null)}
        onConfirm={() => void handleConfirmDelete()}
        title={t("common.tips")}
        description={t("configCenter.messages.deleteServiceConfigConfirm", {
          serviceName: deleteTarget?.service_name ?? "",
        })}
        confirmLabel={t("common.confirm")}
        cancelLabel={t("common.cancel")}
        danger
        loading={deleting}
      />
    </>
  );
}
