import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Plus, Shield, Trash2, Upload } from "lucide-react";
import {
  addToWhitelist,
  fetchTenantWhitelist,
  removeFromWhitelist,
} from "@/api/tenant";
import { CsvImportModal } from "@/components/common/CsvImportModal";
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
  EntityDetailCard,
  FormField,
  FormModal,
  Input,
  StatusBadge,
  Textarea,
  type DataTableColumn,
} from "@/components/ui";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import type { AddToWhitelistRequest, WhitelistEntry } from "@/types/api/tenant";
import {
  EMPTY_PLACEHOLDER,
  formatDateTime,
  formatDisplayDateYmd,
} from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { isBizRole } from "@/utils/ops/roles";
import { exportResourceCsv, type ColumnConfig } from "@/utils/resource-csv";
import { logger } from "@/utils/sys/logger";
import {
  showSuccessToast,
  showWarningToast,
} from "@/utils/ui/message-guard";

export interface WhitelistSectionProps {
  tenantCode: string;
}

const IPV4_RE = /^(\d{1,3}\.){3}\d{1,3}$/;
const IPV4_CIDR_RE = /^(\d{1,3}\.){3}\d{1,3}(\/\d{1,2})?$/;

function isValidIpv4(ip: string): boolean {
  if (!IPV4_RE.test(ip)) return false;
  return ip.split(".").every((part) => {
    const n = Number.parseInt(part, 10);
    return n >= 0 && n <= 255;
  });
}

function formatIpWithCidr(row: WhitelistEntry): string {
  const ip = row.ip_address.split("/")[0];
  return row.cidr === 32 ? ip : `${ip}/${row.cidr}`;
}

function entryKey(row: WhitelistEntry): string {
  return `${row.ip_address.split("/")[0]}/${row.cidr}`;
}

export function WhitelistSection({ tenantCode }: WhitelistSectionProps) {
  const { t } = useT();
  const role = useAuthStore((s) => s.info.role);
  const canWrite = isBizRole(role);

  const [loading, setLoading] = useState(false);
  const [entries, setEntries] = useState<WhitelistEntry[]>([]);
  const [selectedIds, setSelectedIds] = useState<string[]>([]);

  const [addOpen, setAddOpen] = useState(false);
  const [adding, setAdding] = useState(false);
  const [ipText, setIpText] = useState("");
  const [descText, setDescText] = useState("");
  const [addErrors, setAddErrors] = useState<{
    ip_addresses?: string;
    description?: string;
  }>({});

  const [importOpen, setImportOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<WhitelistEntry | null>(null);
  const [batchDeleteKeys, setBatchDeleteKeys] = useState<string[] | null>(null);
  const [deleting, setDeleting] = useState(false);

  const abortRef = useRef<AbortController | null>(null);

  const loadData = useCallback(async () => {
    if (!tenantCode) return;
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;
    setLoading(true);
    try {
      const res = await fetchTenantWhitelist(tenantCode, controller.signal);
      if (controller.signal.aborted) return;
      setEntries(res.entries ?? []);
      setSelectedIds([]);
    } catch (error) {
      if (!controller.signal.aborted && !isMfaStepUpCancelled(error)) {
        logger.error("Failed to load whitelist:", error);
      }
    } finally {
      if (!controller.signal.aborted) {
        setLoading(false);
      }
    }
  }, [tenantCode]);

  useEffect(() => {
    void loadData();
    return () => abortRef.current?.abort();
  }, [loadData]);

  const selectedRows = useMemo(
    () => entries.filter((row) => selectedIds.includes(entryKey(row))),
    [entries, selectedIds],
  );

  const getCidrLabel = useCallback(
    (cidr: number) => {
      if (cidr === 32) return t("tenant.whitelist.cidrLabels.singleIp");
      if (cidr >= 24) return t("tenant.whitelist.cidrLabels.cSegment", { cidr });
      if (cidr >= 16) return t("tenant.whitelist.cidrLabels.bSegment", { cidr });
      return t("tenant.whitelist.cidrLabels.aSegment", { cidr });
    },
    [t],
  );

  const getCidrBadgeVariant = (
    cidr: number,
  ): "success" | "warning" | "neutral" => {
    if (cidr === 32) return "success";
    if (cidr >= 24) return "warning";
    return "neutral";
  };

  const exportItems = useMemo<ExportDropdownItem[]>(
    () => [
      {
        command: "all",
        label: t("tenant.whitelist.exportAll"),
        disabled: entries.length === 0,
      },
      {
        command: "selected",
        label: t("tenant.whitelist.exportSelected", {
          count: selectedRows.length,
        }),
        disabled: selectedRows.length === 0,
      },
    ],
    [entries.length, selectedRows.length, t],
  );

  const handleExport = useCallback(
    (command: string) => {
      const target = command === "selected" ? selectedRows : entries;
      if (target.length === 0) {
        showWarningToast(t("feedback.import.noDataToExport"));
        return;
      }
      const rows = target.map((entry) => {
        const ip = entry.ip_address.split("/")[0];
        return {
          ip_address: entry.cidr === 32 ? ip : `${ip}/${entry.cidr}`,
          cidr: entry.cidr,
          description: entry.description || "",
        };
      });
      const date = formatDisplayDateYmd();
      exportResourceCsv(
        rows,
        [
          { key: "ip_address", label: "ip_address" },
          { key: "cidr", label: "cidr" },
          { key: "description", label: "description" },
        ],
        `ip_whitelist_${tenantCode}_${date}.csv`,
      );
    },
    [entries, selectedRows, tenantCode, t],
  );

  const whitelistImportColumns = useMemo<ColumnConfig[]>(
    () => [
      {
        key: "ip_address",
        label: t("tenant.whitelist.columns.ipAddress"),
        required: true,
      },
      {
        key: "cidr",
        label: t("tenant.whitelist.columns.cidr"),
        required: true,
        type: "number",
      },
      {
        key: "description",
        label: t("tenant.whitelist.columns.description"),
        required: false,
      },
    ],
    [t],
  );

  const whitelistImportFormatHint = useMemo(
    () =>
      [
        t("tenant.whitelist.importDialog.formatHint"),
        t("tenant.whitelist.importDialog.formatExample"),
      ].join("\n"),
    [t],
  );

  const whitelistExistingData = useMemo(
    () => entries.map((entry) => ({ key: entryKey(entry) })),
    [entries],
  );

  const validateImportRow = useCallback(
    (row: Record<string, string>) => {
      const ip = (row.ip_address || "").trim();
      const cidr = Number.parseInt((row.cidr || "").trim() || "32", 10);
      const normalizedIp = ip.split("/")[0];
      if (!isValidIpv4(normalizedIp)) {
        return {
          valid: false,
          reason: t("tenant.whitelist.validation.invalidFormat", {
            entry: ip || normalizedIp,
          }),
        };
      }
      if (!Number.isFinite(cidr) || cidr < 0 || cidr > 32) {
        return {
          valid: false,
          reason: t("tenant.whitelist.validation.cidrRange", {
            entry: ip || normalizedIp,
          }),
        };
      }
      return { valid: true };
    },
    [t],
  );

  const transformImportRow = useCallback((row: Record<string, string>) => {
    const ip = (row.ip_address || "").trim();
    const cidr = Number.parseInt((row.cidr || "").trim() || "32", 10);
    const normalizedIp = ip.split("/")[0];
    const safeCidr = Number.isNaN(cidr) ? 32 : cidr;
    return {
      key: `${normalizedIp}/${safeCidr}`,
      ip_address: normalizedIp,
      cidr: safeCidr,
      description: (row.description || "").trim(),
    };
  }, []);

  const handleWhitelistImport = useCallback(
    async (items: Record<string, unknown>[]) => {
      const typed = items as Array<{
        ip_address: string;
        cidr: number;
        description?: string;
      }>;
      const ipList = typed.map(
        (item) => `${item.ip_address.split("/")[0]}/${item.cidr}`,
      );
      const commonDesc =
        typed.find((item) => item.description)?.description ||
        t("tenant.whitelist.importDialog.defaultDescription");
      const res = await addToWhitelist(tenantCode, {
        ip_addresses: ipList,
        description: commonDesc,
      });
      if (res.total_added > 0) {
        showSuccessToast(
          t("tenant.whitelist.importDialog.importSuccess", {
            count: res.total_added,
          }),
        );
      }
      if (res.total_errors > 0) {
        showWarningToast(
          t("tenant.whitelist.importDialog.importFailedCount", {
            count: res.total_errors,
          }),
        );
      }
    },
    [tenantCode, t],
  );

  const validateAddForm = (): string[] | null => {
    const nextErrors: { ip_addresses?: string; description?: string } = {};
    const lines = ipText
      .trim()
      .split("\n")
      .map((l) => l.trim())
      .filter(Boolean);

    if (lines.length === 0) {
      nextErrors.ip_addresses = t("tenant.whitelist.validation.ipRequired");
    } else {
      for (const entry of lines) {
        if (!IPV4_CIDR_RE.test(entry)) {
          nextErrors.ip_addresses = t(
            "tenant.whitelist.validation.invalidFormat",
            { entry },
          );
          break;
        }
        const [ipPart, cidrPart] = entry.split("/");
        if (!isValidIpv4(ipPart)) {
          nextErrors.ip_addresses = t("tenant.whitelist.validation.ipRange", {
            entry,
          });
          break;
        }
        if (cidrPart !== undefined) {
          const cidr = Number.parseInt(cidrPart, 10);
          if (cidr < 0 || cidr > 32) {
            nextErrors.ip_addresses = t(
              "tenant.whitelist.validation.cidrRange",
              { entry },
            );
            break;
          }
        }
      }
    }

    if (!descText.trim()) {
      nextErrors.description = t(
        "tenant.whitelist.validation.descriptionRequired",
      );
    }

    setAddErrors(nextErrors);
    return Object.keys(nextErrors).length === 0 ? lines : null;
  };

  const handleSubmitAdd = async () => {
    if (adding) return;
    const ipList = validateAddForm();
    if (!ipList) return;
    setAdding(true);
    try {
      const payload: AddToWhitelistRequest = {
        ip_addresses: ipList,
        description: descText.trim() || undefined,
      };
      const res = await addToWhitelist(tenantCode, payload);
      if (res.total_added > 0) {
        showSuccessToast(
          t("tenant.messages.whitelistAddedCount", { count: res.total_added }),
        );
      }
      if (res.total_errors > 0) {
        const details = res.errors
          .map((e) => `${e.ip_address}: ${e.error}`)
          .join("\n");
        showWarningToast(
          t("tenant.messages.whitelistPartialFailure", {
            count: res.total_errors,
            details,
          }),
          5000,
        );
      }
      setAddOpen(false);
      void loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to add whitelist:", error);
      }
    } finally {
      setAdding(false);
    }
  };

  const handleConfirmSingleDelete = async () => {
    if (!deleteTarget || deleting) return;
    setDeleting(true);
    try {
      await removeFromWhitelist(tenantCode, entryKey(deleteTarget));
      showSuccessToast(t("common.feedback.deleteSuccess"));
      setDeleteTarget(null);
      void loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to delete whitelist entry:", error);
      }
    } finally {
      setDeleting(false);
    }
  };

  const handleConfirmBatchDelete = async () => {
    if (!batchDeleteKeys || batchDeleteKeys.length === 0 || deleting) return;
    setDeleting(true);
    let deletedCount = 0;
    try {
      for (const ipIdentifier of batchDeleteKeys) {
        try {
          await removeFromWhitelist(tenantCode, ipIdentifier);
          deletedCount += 1;
        } catch (error) {
          if (isMfaStepUpCancelled(error)) break;
          logger.error(`Failed to delete whitelist ${ipIdentifier}:`, error);
        }
      }
      if (deletedCount > 0) {
        showSuccessToast(t("common.feedback.deleteSuccess"));
        setSelectedIds([]);
        void loadData();
      }
      setBatchDeleteKeys(null);
    } finally {
      setDeleting(false);
    }
  };

  const columns = useMemo<DataTableColumn<WhitelistEntry>[]>(
    () => [
      {
        id: "ip_address",
        header: t("tenant.whitelist.columns.ipAddress"),
        minWidth: 160,
        cell: (row) => <CopyableText value={formatIpWithCidr(row)} mono />,
      },
      {
        id: "cidr",
        header: t("tenant.whitelist.columns.type"),
        width: 120,
        cell: (row) => (
          <Badge variant={getCidrBadgeVariant(row.cidr)}>
            {getCidrLabel(row.cidr)}
          </Badge>
        ),
      },
      {
        id: "description",
        header: t("tenant.whitelist.columns.description"),
        minWidth: 120,
        cell: (row) => row.description || EMPTY_PLACEHOLDER,
      },
      {
        id: "is_active",
        header: t("tenant.whitelist.columns.status"),
        width: 96,
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
      {
        id: "created_at",
        header: t("common.fields.createdAt"),
        width: 168,
        mono: true,
        cell: (row) => formatDateTime(row.created_at),
      },
      ...(canWrite
        ? [
            {
              id: "operations",
              header: t("common.fields.operations"),
              width: 88,
              align: "right" as const,
              cell: (row: WhitelistEntry) => (
                <Button
                  type="button"
                  variant="link"
                  size="xs"
                  className="text-danger-fg hover:text-danger-fg"
                  onClick={() => setDeleteTarget(row)}
                >
                  {t("common.actions.delete")}
                </Button>
              ),
            },
          ]
        : []),
    ],
    [canWrite, getCidrLabel, t],
  );

  const isCompactEmpty = !loading && entries.length === 0;

  return (
    <EntityDetailCard
      title={t("tenant.detail.tabs.whitelist")}
      icon={<Shield className="h-4 w-4" />}
      bodyClassName={isCompactEmpty ? "px-4 py-3.5" : "p-0"}
      testId="tenant-whitelist-card"
      actions={
        isCompactEmpty && canWrite ? (
          <>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => setImportOpen(true)}
            >
              <Upload className="h-3.5 w-3.5" />
              {t("tenant.whitelist.import")}
            </Button>
            <Button
              type="button"
              variant="primary"
              size="sm"
              onClick={() => {
                setIpText("");
                setDescText("");
                setAddErrors({});
                setAddOpen(true);
              }}
            >
              <Plus className="h-3.5 w-3.5" />
              {t("tenant.whitelist.add")}
            </Button>
          </>
        ) : undefined
      }
    >
      {isCompactEmpty ? (
        <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
          <span>
            {t("tenant.whitelist.emptyHint1")} · {t("tenant.whitelist.emptyHint2")}
          </span>
        </div>
      ) : (
        <DataTablePage<WhitelistEntry>
          className="[&>div]:border-0 [&>div]:rounded-none [&>div]:shadow-none"
          columns={columns}
          data={entries}
          getRowId={entryKey}
          loading={loading}
          selectable={canWrite}
          selectedKeys={selectedIds}
          onSelectionChange={setSelectedIds}
          batchActions={
            canWrite
              ? (ids) => (
                  <Button
                    type="button"
                    variant="danger"
                    size="sm"
                    onClick={() => setBatchDeleteKeys(ids)}
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                    {t("tenant.whitelist.batchDelete")}
                  </Button>
                )
              : undefined
          }
          actions={
            <>
              <ExportDropdown
                items={exportItems}
                label={t("tenant.whitelist.export")}
                disabled={entries.length === 0}
                onCommand={handleExport}
              />
              {canWrite && (
                <>
                  <Button
                    type="button"
                    variant="outline"
                    size="md"
                    onClick={() => setImportOpen(true)}
                  >
                    <Upload className="h-3.5 w-3.5" />
                    {t("tenant.whitelist.import")}
                  </Button>
                  <Button
                    type="button"
                    variant="primary"
                    size="md"
                    onClick={() => {
                      setIpText("");
                      setDescText("");
                      setAddErrors({});
                      setAddOpen(true);
                    }}
                  >
                    <Plus className="h-3.5 w-3.5" />
                    {t("tenant.whitelist.add")}
                  </Button>
                </>
              )}
            </>
          }
        />
      )}

      <FormModal
        open={addOpen}
        onClose={() => setAddOpen(false)}
        onSubmit={handleSubmitAdd}
        title={t("tenant.whitelist.addDialog.title")}
        size="md"
        submitLabel={t("tenant.whitelist.addDialog.confirm")}
        submitting={adding}
      >
        <FormField
          label={t("tenant.whitelist.addDialog.ipAddresses")}
          htmlFor="whitelist-ip-input"
          required
          error={addErrors.ip_addresses}
          hint={t("tenant.whitelist.addDialog.ipAddressesHint")}
        >
          <Textarea
            id="whitelist-ip-input"
            rows={4}
            value={ipText}
            onChange={(e) => {
              setIpText(e.target.value);
              if (addErrors.ip_addresses) {
                setAddErrors((prev) => ({ ...prev, ip_addresses: undefined }));
              }
            }}
            placeholder={t("tenant.whitelist.addDialog.ipAddressesPlaceholder")}
            className="font-mono-tech"
          />
        </FormField>
        <FormField
          label={t("tenant.whitelist.addDialog.description")}
          htmlFor="whitelist-desc-input"
          required
          error={addErrors.description}
        >
          <Input
            id="whitelist-desc-input"
            maxLength={100}
            value={descText}
            onChange={(e) => {
              setDescText(e.target.value);
              if (addErrors.description) {
                setAddErrors((prev) => ({ ...prev, description: undefined }));
              }
            }}
            placeholder={t("tenant.whitelist.addDialog.descriptionPlaceholder")}
          />
        </FormField>
      </FormModal>

      <CsvImportModal
        open={importOpen}
        onClose={() => setImportOpen(false)}
        title={t("tenant.whitelist.importDialog.title")}
        columns={whitelistImportColumns}
        formatHint={whitelistImportFormatHint}
        uniqueKey="key"
        existingData={whitelistExistingData}
        validator={validateImportRow}
        transformer={transformImportRow}
        onImport={handleWhitelistImport}
        showSuccessMessage={false}
        showErrorMessage={false}
        onSuccess={() => void loadData()}
      />

      <ConfirmDialog
        open={Boolean(deleteTarget)}
        onClose={() => setDeleteTarget(null)}
        onConfirm={() => void handleConfirmSingleDelete()}
        loading={deleting}
        danger
        title={t("feedback.batch.deleteConfirmTitle")}
        confirmLabel={t("common.actions.delete")}
        description={
          deleteTarget
            ? t("tenant.messages.deleteIpConfirm", {
                ip: entryKey(deleteTarget),
              })
            : ""
        }
      />

      <ConfirmDialog
        open={Boolean(batchDeleteKeys && batchDeleteKeys.length > 0)}
        onClose={() => setBatchDeleteKeys(null)}
        onConfirm={() => void handleConfirmBatchDelete()}
        loading={deleting}
        danger
        title={t("feedback.batch.deleteConfirmTitle")}
        confirmLabel={t("common.actions.delete")}
        description={
          batchDeleteKeys
            ? t("tenant.whitelist.deleteConfirmSelected", {
                count: batchDeleteKeys.length,
              })
            : ""
        }
      />
    </EntityDetailCard>
  );
}
