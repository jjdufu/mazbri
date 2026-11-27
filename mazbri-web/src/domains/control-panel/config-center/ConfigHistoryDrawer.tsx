import { useCallback, useEffect, useMemo, useState } from "react";
import { Info, Loader2, RotateCcw } from "lucide-react";
import { fetchConfigHistory, rollbackConfig } from "@/api/config-center";
import {
  AuthorizationTooltipButton,
  Button,
  CodeEditor,
  ConfirmDialog,
  DiffViewer,
  Drawer,
  Pagination,
  Select,
  cn,
} from "@/components/ui";
import { useT } from "@/locales";
import type { ConfigHistory, ConfigSummary } from "@/types/api/config-center";
import { formatDateTime } from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { logger } from "@/utils/sys/logger";
import {
  showSuccessToast,
  showWarningToast,
} from "@/utils/ui/message-guard";

export interface ConfigHistoryInitialData {
  config: ConfigSummary;
  activeContent: string;
  history: ConfigHistory[];
  total: number;
}

export interface ConfigHistoryDrawerProps {
  open: boolean;
  initialData: ConfigHistoryInitialData | null;
  authorizationDisabled: boolean;
  authorizationDisabledReason: string;
  onClose: () => void;
  onRollbackSuccess: () => void;
}

export function ConfigHistoryDrawer({
  open,
  initialData,
  authorizationDisabled,
  authorizationDisabledReason,
  onClose,
  onRollbackSuccess,
}: ConfigHistoryDrawerProps) {
  const { t } = useT();

  const [activeContent, setActiveContent] = useState("");
  const [historyData, setHistoryData] = useState<ConfigHistory[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(10);
  const [loadingPage, setLoadingPage] = useState(false);

  const [selectedVersion, setSelectedVersion] = useState<ConfigHistory | null>(
    null,
  );
  const [compareTarget, setCompareTarget] = useState<string>("current");
  const [rollbackTarget, setRollbackTarget] = useState<ConfigHistory | null>(
    null,
  );
  const [rollingBack, setRollingBack] = useState(false);

  useEffect(() => {
    if (!open || !initialData) {
      setRollbackTarget(null);
      return;
    }
    setActiveContent(initialData.activeContent);
    setHistoryData(initialData.history);
    setTotal(initialData.total);
    setPage(1);
    setPageSize(10);
    setSelectedVersion(initialData.history[0] ?? null);
    setCompareTarget("current");
  }, [open, initialData]);

  const loadPage = useCallback(
    async (nextPage: number, nextSize: number) => {
      if (!initialData) return;
      setLoadingPage(true);
      try {
        const res = await fetchConfigHistory({
          service_name: initialData.config.service_name,
          page: nextPage,
          page_size: nextSize,
        });
        setHistoryData(res.history ?? []);
        setTotal(res.total ?? 0);
        setSelectedVersion(res.history?.[0] ?? null);
        setCompareTarget("current");
      } catch (error) {
        if (!isMfaStepUpCancelled(error)) {
          logger.error("Failed to load config history page:", error);
        }
      } finally {
        setLoadingPage(false);
      }
    },
    [initialData],
  );

  const compareOptions = useMemo(() => {
    const opts = [
      {
        value: "current",
        label: t("configCenter.historyDrawer.compare.currentActive"),
        text: t("configCenter.historyDrawer.compare.currentActive"),
      },
    ];
    for (const h of historyData) {
      if (selectedVersion && h.version_number === selectedVersion.version_number) {
        continue;
      }
      const label = t("configCenter.historyDrawer.compare.versionLabel", {
        version: h.version_number,
      });
      opts.push({ value: String(h.version_number), label, text: label });
    }
    return opts;
  }, [historyData, selectedVersion, t]);

  const comparedContent = useMemo(() => {
    if (compareTarget === "current") return activeContent;
    const targetVer = Number(compareTarget);
    const found = historyData.find((h) => h.version_number === targetVer);
    return found ? found.content : activeContent;
  }, [compareTarget, activeContent, historyData]);

  const comparedLabel = useMemo(() => {
    if (compareTarget === "current") {
      return t("configCenter.diffViewer.currentActive");
    }
    return t("configCenter.diffViewer.version", { version: compareTarget });
  }, [compareTarget, t]);

  const handleConfirmRollback = async () => {
    if (!rollbackTarget || !initialData || rollingBack) return;
    if (authorizationDisabled) {
      showWarningToast(authorizationDisabledReason);
      return;
    }
    setRollingBack(true);
    try {
      await rollbackConfig({
        service_name: initialData.config.service_name,
        version: rollbackTarget.version_number,
      });
      showSuccessToast(
        t("configCenter.historyDrawer.messages.rollbackSuccess"),
      );
      setRollbackTarget(null);
      onClose();
      onRollbackSuccess();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to rollback config:", error);
      }
    } finally {
      setRollingBack(false);
    }
  };

  return (
    <>
      <Drawer
        open={open}
        onClose={onClose}
        size="lg"
        testId="config-history-drawer"
        title={t("configCenter.historyDrawer.title")}
        subtitle={initialData?.config.service_name || undefined}
        footer={
          <div className="flex items-center justify-end gap-2 w-full">
            {selectedVersion && (
              <AuthorizationTooltipButton
                type="button"
                variant="primary"
                disabled={authorizationDisabled}
                disabledReason={authorizationDisabledReason}
                onClick={() => setRollbackTarget(selectedVersion)}
              >
                <RotateCcw className="h-3.5 w-3.5" />
                {t("configCenter.historyDrawer.actions.rollbackToSelected")}
              </AuthorizationTooltipButton>
            )}
            <Button type="button" variant="outline" onClick={onClose}>
              {t("common.actions.close")}
            </Button>
          </div>
        }
      >
        {historyData.length === 0 ? (
          <div className="space-y-4">
            <div className="flex items-center gap-2 rounded-lg border border-primary/30 bg-primary-soft/50 px-3 py-2.5 text-xs text-primary">
              <Info className="h-4 w-4 shrink-0" />
              <span>
                {t("configCenter.historyDrawer.initialVersionNotice")}
              </span>
            </div>
            <div className="space-y-2">
              <h4 className="text-xs font-semibold text-foreground">
                {t("configCenter.historyDrawer.activeConfigTitle")}
              </h4>
              <CodeEditor
                value={activeContent}
                language="toml"
                readOnly
                minHeight={400}
              />
            </div>
          </div>
        ) : (
          <div className="space-y-4">
            <div className="relative overflow-hidden rounded-lg border border-border">
              {loadingPage && (
                <div className="absolute inset-0 z-10 flex items-center justify-center bg-card/65">
                  <Loader2 className="h-4 w-4 animate-spin text-primary" />
                </div>
              )}
              <div className="max-h-60 overflow-y-auto">
                <table className="w-full border-collapse text-xs">
                  <thead>
                    <tr className="border-b border-border bg-card text-muted-foreground sticky top-0">
                      <th className="h-8 px-3 text-left font-medium w-20">
                        {t("configCenter.historyDrawer.columns.version")}
                      </th>
                      <th className="h-8 px-3 text-left font-medium">
                        {t("configCenter.historyDrawer.columns.updatedBy")}
                      </th>
                      <th className="h-8 px-3 text-left font-medium">
                        {t("configCenter.historyDrawer.columns.createdAt")}
                      </th>
                      <th className="h-8 px-3 text-right font-medium w-24">
                        {t("common.fields.operations")}
                      </th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-border-subtle">
                    {historyData.map((row) => {
                      const active =
                        selectedVersion?.version_number === row.version_number;
                      return (
                        <tr
                          key={row.id}
                          onClick={() => {
                            setSelectedVersion(row);
                            if (compareTarget === String(row.version_number)) {
                              setCompareTarget("current");
                            }
                          }}
                          className={cn(
                            "transition-colors cursor-pointer",
                            active
                              ? "bg-primary-soft/60"
                              : "hover:bg-hover-bg/75",
                          )}
                        >
                          <td className="px-3 py-2 font-mono-tech font-medium tabular-nums">
                            #{row.version_number}
                          </td>
                          <td className="px-3 py-2 text-foreground">
                            {row.updated_by || "—"}
                          </td>
                          <td className="px-3 py-2 font-mono-tech tabular-nums text-muted-foreground">
                            {formatDateTime(row.created_at)}
                          </td>
                          <td
                            className="px-3 py-2 text-right"
                            onClick={(e) => e.stopPropagation()}
                          >
                            <AuthorizationTooltipButton
                              type="button"
                              variant="link"
                              size="xs"
                              disabled={authorizationDisabled}
                              disabledReason={authorizationDisabledReason}
                              onClick={() => setRollbackTarget(row)}
                            >
                              {t("configCenter.historyDrawer.actions.rollback")}
                            </AuthorizationTooltipButton>
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
              {total > pageSize && (
                <div className="border-t border-border px-3 py-2">
                  <Pagination
                    page={page}
                    pageSize={pageSize}
                    total={total}
                    onPageChange={(next) => {
                      setPage(next);
                      void loadPage(next, pageSize);
                    }}
                    pageSizeOptions={[10, 20]}
                    onPageSizeChange={(nextSize) => {
                      setPageSize(nextSize);
                      setPage(1);
                      void loadPage(1, nextSize);
                    }}
                  />
                </div>
              )}
            </div>

            {selectedVersion && (
              <div className="space-y-2.5">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div className="flex items-center gap-2">
                    <span className="text-xs font-medium text-muted-foreground">
                      {t("configCenter.historyDrawer.compare.title")}
                    </span>
                    <Select
                      value={compareTarget}
                      onChange={(e) => setCompareTarget(e.target.value)}
                      options={compareOptions}
                      className="w-44"
                    />
                  </div>
                </div>

                <DiffViewer
                  oldValue={selectedVersion.content}
                  newValue={comparedContent}
                  fileName={`${initialData?.config.service_name ?? "config"}.toml`}
                  oldLabel={t("configCenter.diffViewer.version", {
                    version: selectedVersion.version_number,
                  })}
                  newLabel={comparedLabel}
                  outputFormat="line-by-line"
                />
              </div>
            )}
          </div>
        )}
      </Drawer>

      <ConfirmDialog
        open={rollbackTarget !== null}
        onClose={() => setRollbackTarget(null)}
        onConfirm={() => void handleConfirmRollback()}
        title={t("configCenter.historyDrawer.confirmTitle")}
        description={t("configCenter.historyDrawer.confirmMessage", {
          version: rollbackTarget?.version_number ?? "",
        })}
        confirmLabel={t("common.confirm")}
        cancelLabel={t("common.cancel")}
        danger
        loading={rollingBack}
      />
    </>
  );
}
