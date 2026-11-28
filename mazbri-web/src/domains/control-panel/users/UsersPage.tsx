import { useCallback, useEffect, useMemo, useState } from "react";
import { Download, Plus, RefreshCw, Trash2 } from "lucide-react";
import { deleteAdmin, fetchAdminList } from "@/api/admin";
import {
  Badge,
  Button,
  ConfirmDialog,
  DataTablePage,
  ListQueryBar,
  StatusBadge,
  type DataTableColumn,
} from "@/components/ui";
import { usersCatalog } from "@/list-catalog/common-resources";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import type { AdminUser } from "@/types/api/admin";
import type { ListFilter } from "@/types/list-query";
import { downloadCsv, generateCsv } from "@/utils/csv";
import { getRoleLabel } from "@/utils/display-labels";
import { EMPTY_PLACEHOLDER, formatDateTime } from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { encodeFilters } from "@/utils/list-query/serialize";
import { logger } from "@/utils/sys/logger";
import {
  showSuccessToast,
  showWarningToast,
} from "@/utils/ui/message-guard";
import { UserCreateModal } from "./UserCreateModal";
import { UserDetailDrawer } from "./UserDetailDrawer";

function roleBadgeVariant(role: string): "danger" | "xblue" | "neutral" {
  if (role === "R_ADMIN") return "danger";
  if (role === "R_FINANCE") return "xblue";
  return "neutral";
}

export function UsersPage() {
  const { t } = useT();
  const currentUserId = useAuthStore((s) => String(s.info.id ?? ""));

  const [rows, setRows] = useState<AdminUser[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(10);
  const [loading, setLoading] = useState(false);

  const [qInput, setQInput] = useState("");
  const [appliedQ, setAppliedQ] = useState("");
  const [listFilters, setListFilters] = useState<ListFilter[]>([]);

  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [createOpen, setCreateOpen] = useState(false);
  const [detailOpen, setDetailOpen] = useState(false);
  const [selectedAdmin, setSelectedAdmin] = useState<AdminUser | null>(null);

  const [deleteTarget, setDeleteTarget] = useState<AdminUser | null>(null);
  const [batchDeleteIds, setBatchDeleteIds] = useState<string[] | null>(null);
  const [deleting, setDeleting] = useState(false);

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const res = await fetchAdminList({
        page,
        page_size: pageSize,
        q: appliedQ || undefined,
        filters:
          listFilters.length > 0
            ? encodeFilters(listFilters).filters
            : undefined,
      });
      setRows(res.items ?? []);
      setTotal(res.total ?? 0);
      setSelectedIds([]);
    } catch (error) {
      logger.error("Failed to fetch admin list:", error);
    } finally {
      setLoading(false);
    }
  }, [page, pageSize, appliedQ, listFilters]);

  useEffect(() => {
    void loadData();
  }, [loadData]);

  const handleSearch = useCallback(() => {
    setPage(1);
    setAppliedQ(qInput.trim());
  }, [qInput]);

  const handleFiltersChange = useCallback((next: ListFilter[]) => {
    setPage(1);
    setListFilters(next);
  }, []);

  const handleExportCsv = () => {
    const targetRows =
      selectedIds.length > 0
        ? rows.filter((row) => selectedIds.includes(row.id))
        : rows;
    if (targetRows.length === 0) return;

    const exportData = targetRows.map((row) => ({
      username: row.username,
      display_name: row.display_name,
      email: row.email ?? "",
      role: row.role,
      status: row.is_active !== false ? "active" : "inactive",
      mfa_enabled: row.mfa_enabled,
      mfa_required: row.mfa_required,
      force_password_reset: row.force_password_reset,
      created_at: formatDateTime(row.created_at),
    }));

    const csv = generateCsv(exportData, [
      { key: "username", label: t("userManagement.list.columns.username") },
      {
        key: "display_name",
        label: t("userManagement.list.columns.displayName"),
      },
      { key: "email", label: t("userManagement.list.columns.email") },
      { key: "role", label: t("userManagement.list.columns.role") },
      { key: "status", label: t("userManagement.list.columns.status") },
      { key: "mfa_enabled", label: t("userManagement.list.columns.mfa") },
      {
        key: "mfa_required",
        label: t("userManagement.list.columns.mfaRequired"),
      },
      {
        key: "force_password_reset",
        label: t("userManagement.createDialog.fields.firstLogin"),
      },
      { key: "created_at", label: t("userManagement.list.columns.createdAt") },
    ]);
    const scope = selectedIds.length > 0 ? "selected" : "page";
    downloadCsv(csv, `users-${scope}-${Date.now().toString(36)}.csv`);
  };

  const handleConfirmSingleDelete = async () => {
    if (!deleteTarget || deleting) return;
    if (deleteTarget.role === "R_ADMIN") {
      showWarningToast(t("userManagement.messages.deleteSystemAdminBlocked"));
      setDeleteTarget(null);
      return;
    }
    setDeleting(true);
    try {
      await deleteAdmin(deleteTarget.id);
      showSuccessToast(t("common.feedback.deleteSuccess"));
      setDeleteTarget(null);
      void loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to delete user:", error);
      }
    } finally {
      setDeleting(false);
    }
  };

  const handleConfirmBatchDelete = async () => {
    if (!batchDeleteIds || batchDeleteIds.length === 0 || deleting) return;
    const targets = rows.filter(
      (r) =>
        batchDeleteIds.includes(r.id) &&
        r.role !== "R_ADMIN" &&
        r.id !== currentUserId,
    );
    if (targets.length === 0) {
      showWarningToast(t("userManagement.messages.deleteSystemAdminBlocked"));
      setBatchDeleteIds(null);
      return;
    }
    setDeleting(true);
    let deletedCount = 0;
    try {
      for (const target of targets) {
        try {
          await deleteAdmin(target.id);
          deletedCount += 1;
        } catch (error) {
          if (isMfaStepUpCancelled(error)) {
            break;
          }
          logger.error(`Failed to delete user ${target.username}:`, error);
        }
      }
      if (deletedCount > 0) {
        showSuccessToast(t("common.feedback.deleteSuccess"));
        setSelectedIds([]);
        void loadData();
      }
      setBatchDeleteIds(null);
    } finally {
      setDeleting(false);
    }
  };

  const columns = useMemo<DataTableColumn<AdminUser>[]>(
    () => [
      {
        id: "username",
        header: t("userManagement.list.columns.username"),
        minWidth: 84,
        cell: (row) => (
          <span
            className="block max-w-[96px] truncate font-medium text-foreground"
            title={row.username}
          >
            {row.username}
          </span>
        ),
      },
      {
        id: "display_name",
        header: t("userManagement.list.columns.displayName"),
        minWidth: 84,
        cell: (row) => (
          <span
            className="block max-w-[96px] truncate text-foreground"
            title={row.display_name || undefined}
          >
            {row.display_name || EMPTY_PLACEHOLDER}
          </span>
        ),
      },
      {
        id: "email",
        header: t("userManagement.list.columns.email"),
        minWidth: 140,
        cell: (row) => (
          <span
            className="block max-w-[152px] truncate text-muted-foreground"
            title={row.email || undefined}
          >
            {row.email || EMPTY_PLACEHOLDER}
          </span>
        ),
      },
      {
        id: "role",
        header: t("userManagement.list.columns.role"),
        width: 92,
        className: "px-2",
        headerClassName: "px-2",
        cell: (row) => (
          <Badge variant={roleBadgeVariant(row.role)}>
            {getRoleLabel(row.role)}
          </Badge>
        ),
      },
      {
        id: "status",
        header: t("userManagement.list.columns.status"),
        width: 68,
        align: "center",
        className: "px-2",
        headerClassName: "px-2",
        cell: (row) => (
          <StatusBadge
            status={row.is_active !== false ? "ACTIVE" : "DISABLED"}
            label={
              row.is_active !== false
                ? t("userManagement.status.active")
                : t("userManagement.status.inactive")
            }
          />
        ),
      },
      {
        id: "mfa_enabled",
        header: t("userManagement.list.columns.mfa"),
        width: 74,
        align: "center",
        className: "px-2",
        headerClassName: "px-2",
        cell: (row) => (
          <Badge variant={row.mfa_enabled ? "success" : "neutral"}>
            {row.mfa_enabled
              ? t("userManagement.status.mfaBound")
              : t("userManagement.status.mfaUnbound")}
          </Badge>
        ),
      },
      {
        id: "mfa_required",
        header: t("userManagement.list.columns.mfaRequired"),
        width: 78,
        align: "center",
        className: "px-2",
        headerClassName: "px-2",
        cell: (row) => (
          <Badge variant={row.mfa_required ? "warning" : "neutral"}>
            {row.mfa_required
              ? t("userManagement.status.policyOn")
              : t("userManagement.status.policyOff")}
          </Badge>
        ),
      },
      {
        id: "last_login_at",
        header: t("userManagement.list.columns.lastLogin"),
        minWidth: 132,
        mono: true,
        className: "px-2",
        headerClassName: "px-2",
        cell: (row) => (
          <span className="whitespace-nowrap text-[11px] tabular-nums">
            {row.last_login_at
              ? formatDateTime(row.last_login_at)
              : EMPTY_PLACEHOLDER}
          </span>
        ),
      },
      {
        id: "created_at",
        header: t("userManagement.list.columns.createdAt"),
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
        width: 108,
        align: "right",
        className: "px-2 pr-4",
        headerClassName: "px-2 pr-4",
        cell: (row) => (
          <div
            className="inline-flex items-center justify-end gap-1"
            onClick={(e) => e.stopPropagation()}
          >
            <Button
              type="button"
              variant="link"
              size="xs"
              onClick={() => {
                setSelectedAdmin(row);
                setDetailOpen(true);
              }}
            >
              {t("common.actions.details")}
            </Button>
            {row.role !== "R_ADMIN" && row.id !== currentUserId && (
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
    [t, currentUserId],
  );

  const hasFilters = Boolean(appliedQ) || listFilters.length > 0;

  return (
    <div className="flex flex-col flex-1 min-h-0 min-w-0">
      <DataTablePage<AdminUser>
        testId="users-page"
        columns={columns}
        data={rows}
        getRowId={(row) => row.id}
        loading={loading}
        selectable
        selectedKeys={selectedIds}
        onSelectionChange={setSelectedIds}
        emptyText={
          hasFilters
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
        onRowClick={(row) => {
          setSelectedAdmin(row);
          setDetailOpen(true);
        }}
        filters={
          <ListQueryBar
            q={qInput}
            onQChange={setQInput}
            filters={listFilters}
            onFiltersChange={handleFiltersChange}
            catalog={usersCatalog}
            onSearch={handleSearch}
          />
        }
        batchActions={(ids) => (
          <Button
            type="button"
            variant="danger"
            size="md"
            onClick={() => setBatchDeleteIds(ids)}
            data-testid="users-batch-delete-btn"
          >
            <Trash2 className="h-3.5 w-3.5" />
            {t("common.actions.delete")}
          </Button>
        )}
        actions={
          <>
            <Button
              type="button"
              variant="outline"
              size="md"
              disabled={rows.length === 0}
              onClick={handleExportCsv}
            >
              <Download className="h-3.5 w-3.5" />
              {t("common.actions.export")}
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="md"
              disabled={loading}
              onClick={() => void loadData()}
              aria-label={t("common.actions.refresh")}
            >
              <RefreshCw
                className={loading ? "h-3.5 w-3.5 animate-spin" : "h-3.5 w-3.5"}
              />
              {t("common.actions.refresh")}
            </Button>
            <Button
              type="button"
              variant="primary"
              size="md"
              onClick={() => setCreateOpen(true)}
              data-testid="users-create-btn"
            >
              <Plus className="h-3.5 w-3.5" />
              {t("userManagement.list.addUser")}
            </Button>
          </>
        }
      />

      <UserCreateModal
        open={createOpen}
        onClose={() => setCreateOpen(false)}
        onSuccess={() => void loadData()}
      />

      <UserDetailDrawer
        open={detailOpen}
        adminId={selectedAdmin?.id ?? null}
        initialAdmin={selectedAdmin}
        onClose={() => setDetailOpen(false)}
        onChanged={() => void loadData()}
        onDeleted={() => void loadData()}
      />

      <ConfirmDialog
        open={Boolean(deleteTarget)}
        onClose={() => setDeleteTarget(null)}
        onConfirm={() => void handleConfirmSingleDelete()}
        title={t("userManagement.detail.deleteUser")}
        description={t("userManagement.messages.deleteUserConfirm", {
          username: deleteTarget?.username ?? "",
        })}
        confirmLabel={t("common.confirm")}
        cancelLabel={t("common.cancel")}
        danger
        loading={deleting}
      />

      <ConfirmDialog
        open={Boolean(batchDeleteIds && batchDeleteIds.length > 0)}
        onClose={() => setBatchDeleteIds(null)}
        onConfirm={() => void handleConfirmBatchDelete()}
        title={t("common.actions.delete")}
        description={t("common.confirmDelete")}
        confirmLabel={t("common.confirm")}
        cancelLabel={t("common.cancel")}
        danger
        loading={deleting}
      />
    </div>
  );
}
