import { useCallback, useEffect, useState } from "react";
import { KeyRound, Loader2, Pencil, ShieldOff, Trash2 } from "lucide-react";
import {
  deleteAdmin,
  fetchAdminById,
  resetAdminMfa,
  setAdminStatus,
} from "@/api/admin";
import {
  Badge,
  Button,
  ConfirmDialog,
  CopyableText,
  Drawer,
  StatusBadge,
  Switch,
} from "@/components/ui";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import type { AdminUser } from "@/types/api/admin";
import { getRoleLabel } from "@/utils/display-labels";
import { EMPTY_PLACEHOLDER, formatDateTime } from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { logger } from "@/utils/sys/logger";
import { showSuccessToast } from "@/utils/ui/message-guard";
import { UserEditModal } from "./UserEditModal";
import { UserResetPasswordModal } from "./UserResetPasswordModal";

function roleBadgeVariant(role: string): "danger" | "xblue" | "neutral" {
  if (role === "R_ADMIN") return "danger";
  if (role === "R_FINANCE") return "xblue";
  return "neutral";
}

export interface UserDetailDrawerProps {
  open: boolean;
  adminId: string | null;
  initialAdmin: AdminUser | null;
  onClose: () => void;
  onChanged: () => void;
  onDeleted: () => void;
}

export function UserDetailDrawer({
  open,
  adminId,
  initialAdmin,
  onClose,
  onChanged,
  onDeleted,
}: UserDetailDrawerProps) {
  const { t } = useT();
  const currentUserId = useAuthStore((s) => String(s.info.id ?? ""));

  const [user, setUser] = useState<AdminUser | null>(null);
  const [loading, setLoading] = useState(false);
  const [statusLoading, setStatusLoading] = useState(false);
  const [mfaLoading, setMfaLoading] = useState(false);
  const [deleteLoading, setDeleteLoading] = useState(false);

  const [editOpen, setEditOpen] = useState(false);
  const [resetPwOpen, setResetPwOpen] = useState(false);
  const [confirmMfaOpen, setConfirmMfaOpen] = useState(false);
  const [confirmDeleteOpen, setConfirmDeleteOpen] = useState(false);

  const loadUser = useCallback(async (id: string) => {
    setLoading(true);
    try {
      const detail = await fetchAdminById(id);
      setUser(detail);
    } catch (error) {
      logger.error("Failed to load admin detail:", error);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!open) {
      setEditOpen(false);
      setResetPwOpen(false);
      setConfirmMfaOpen(false);
      setConfirmDeleteOpen(false);
      return;
    }
    if (initialAdmin && initialAdmin.id === adminId) {
      setUser(initialAdmin);
    }
    if (adminId) {
      void loadUser(adminId);
    }
  }, [open, adminId, initialAdmin, loadUser]);

  const isSuperAdminTarget = user?.role === "R_ADMIN";
  const isSelf = Boolean(user?.id) && user?.id === currentUserId;

  const handleStatusChange = async (nextActive: boolean) => {
    if (!user || isSuperAdminTarget || isSelf || statusLoading) return;
    setStatusLoading(true);
    try {
      await setAdminStatus(user.id, { is_active: nextActive });
      setUser((prev) => (prev ? { ...prev, is_active: nextActive } : prev));
      showSuccessToast(
        nextActive
          ? t("common.feedback.enabled")
          : t("common.feedback.disabled"),
      );
      onChanged();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to change admin status:", error);
      }
    } finally {
      setStatusLoading(false);
    }
  };

  const handleConfirmResetMfa = async () => {
    if (!user || isSuperAdminTarget || !user.mfa_enabled || mfaLoading) return;
    setMfaLoading(true);
    try {
      await resetAdminMfa(user.id);
      showSuccessToast(t("common.feedback.mfaReset"));
      setConfirmMfaOpen(false);
      await loadUser(user.id);
      onChanged();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to reset MFA:", error);
      }
    } finally {
      setMfaLoading(false);
    }
  };

  const handleConfirmDelete = async () => {
    if (!user || isSuperAdminTarget || isSelf || deleteLoading) return;
    setDeleteLoading(true);
    try {
      await deleteAdmin(user.id);
      showSuccessToast(t("common.feedback.deleteSuccess"));
      setConfirmDeleteOpen(false);
      onClose();
      onDeleted();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to delete user:", error);
      }
    } finally {
      setDeleteLoading(false);
    }
  };

  const mfaControlHint = isSuperAdminTarget
    ? t("userManagement.detail.controls.mfaProtectedHint")
    : !user?.mfa_enabled
      ? t("userManagement.detail.controls.mfaUnboundHint")
      : t("userManagement.detail.controls.mfaHint");

  return (
    <>
      <Drawer
        open={open}
        onClose={onClose}
        size="md"
        testId="user-detail-drawer"
        title={
          <span className="inline-flex items-center gap-2 min-w-0">
            <span className="truncate">
              {user?.display_name ||
                user?.username ||
                t("userManagement.detail.userTitle")}
            </span>
            {user && (
              <StatusBadge
                status={user.is_active ? "ACTIVE" : "DISABLED"}
                label={
                  user.is_active
                    ? t("userManagement.status.active")
                    : t("userManagement.status.inactive")
                }
              />
            )}
          </span>
        }
        subtitle={user?.email || user?.username || undefined}
        footer={
          <div className="flex items-center justify-end gap-2 w-full">
            <Button type="button" variant="outline" onClick={onClose}>
              {t("common.actions.close")}
            </Button>
            <Button
              type="button"
              variant="primary"
              disabled={!user}
              onClick={() => setEditOpen(true)}
              data-testid="user-detail-edit-btn"
            >
              <Pencil className="h-3.5 w-3.5" />
              {t("userManagement.detail.editAction")}
            </Button>
          </div>
        }
      >
        {loading && !user ? (
          <div className="flex items-center justify-center py-12 text-xs text-muted-foreground gap-2">
            <Loader2 className="h-4 w-4 animate-spin text-primary" />
            <span>{t("layout.loading")}</span>
          </div>
        ) : user ? (
          <div className="space-y-5">
            <section className="space-y-2.5">
              <h3 className="text-xs font-semibold text-foreground">
                {t("userManagement.detail.basicInfo")}
              </h3>
              <dl className="grid grid-cols-1 sm:grid-cols-2 border border-border rounded-lg overflow-hidden divide-y sm:divide-y-0 text-xs">
                <div className="sm:col-span-2 flex flex-col gap-1 p-3 border-b border-border bg-muted/20">
                  <dt className="text-[11px] text-muted-foreground">
                    {t("userManagement.detail.labels.id")}
                  </dt>
                  <dd className="text-foreground">
                    <CopyableText value={user.id} />
                  </dd>
                </div>

                <div className="flex flex-col gap-1 p-3 border-b sm:border-r border-border">
                  <dt className="text-[11px] text-muted-foreground">
                    {t("userManagement.detail.labels.username")}
                  </dt>
                  <dd className="text-foreground">
                    <CopyableText value={user.username} />
                  </dd>
                </div>

                <div className="flex flex-col gap-1 p-3 border-b border-border">
                  <dt className="text-[11px] text-muted-foreground">
                    {t("userManagement.detail.labels.displayName")}
                  </dt>
                  <dd className="text-foreground font-medium">
                    {user.display_name || EMPTY_PLACEHOLDER}
                  </dd>
                </div>

                <div className="sm:col-span-2 flex flex-col gap-1 p-3 border-b border-border">
                  <dt className="text-[11px] text-muted-foreground">
                    {t("userManagement.detail.labels.email")}
                  </dt>
                  <dd className="text-foreground break-all">
                    {user.email || EMPTY_PLACEHOLDER}
                  </dd>
                </div>

                <div className="flex flex-col gap-1 p-3 border-b sm:border-r border-border">
                  <dt className="text-[11px] text-muted-foreground">
                    {t("userManagement.detail.labels.role")}
                  </dt>
                  <dd>
                    <Badge variant={roleBadgeVariant(user.role)}>
                      {getRoleLabel(user.role)}
                    </Badge>
                  </dd>
                </div>

                <div className="flex flex-col gap-1 p-3 border-b border-border">
                  <dt className="text-[11px] text-muted-foreground">
                    {t("userManagement.detail.labels.status")}
                  </dt>
                  <dd>
                    <StatusBadge
                      status={user.is_active ? "ACTIVE" : "DISABLED"}
                      label={
                        user.is_active
                          ? t("userManagement.status.active")
                          : t("userManagement.status.inactive")
                      }
                    />
                  </dd>
                </div>

                <div className="flex flex-col gap-1 p-3 border-b sm:border-r border-border">
                  <dt className="text-[11px] text-muted-foreground">
                    {t("userManagement.detail.labels.mfaStatus")}
                  </dt>
                  <dd>
                    <Badge variant={user.mfa_enabled ? "success" : "neutral"}>
                      {user.mfa_enabled
                        ? t("userManagement.status.mfaBound")
                        : t("userManagement.status.mfaUnbound")}
                    </Badge>
                  </dd>
                </div>

                <div className="flex flex-col gap-1 p-3 border-b border-border">
                  <dt className="text-[11px] text-muted-foreground">
                    {t("userManagement.detail.labels.mfaRequired")}
                  </dt>
                  <dd>
                    <Badge variant={user.mfa_required ? "warning" : "neutral"}>
                      {user.mfa_required
                        ? t("userManagement.status.policyOn")
                        : t("userManagement.status.policyOff")}
                    </Badge>
                  </dd>
                </div>

                <div className="flex flex-col gap-1 p-3 border-b sm:border-r border-border">
                  <dt className="text-[11px] text-muted-foreground">
                    {t("userManagement.detail.labels.forcePasswordReset")}
                  </dt>
                  <dd>
                    <Badge
                      variant={
                        user.force_password_reset ? "warning" : "neutral"
                      }
                    >
                      {user.force_password_reset
                        ? t("userManagement.status.policyOn")
                        : t("userManagement.status.policyOff")}
                    </Badge>
                  </dd>
                </div>

                <div className="flex flex-col gap-1 p-3 border-b border-border">
                  <dt className="text-[11px] text-muted-foreground">
                    {t("userManagement.detail.labels.passwordChangedAt")}
                  </dt>
                  <dd className="font-mono-tech tabular-nums text-foreground">
                    {user.password_changed_at
                      ? formatDateTime(user.password_changed_at)
                      : EMPTY_PLACEHOLDER}
                  </dd>
                </div>

                <div className="flex flex-col gap-1 p-3 border-b sm:border-r border-border">
                  <dt className="text-[11px] text-muted-foreground">
                    {t("userManagement.detail.labels.lastLogin")}
                  </dt>
                  <dd className="font-mono-tech tabular-nums text-foreground">
                    {user.last_login_at
                      ? formatDateTime(user.last_login_at)
                      : EMPTY_PLACEHOLDER}
                  </dd>
                </div>

                <div className="flex flex-col gap-1 p-3 border-b border-border">
                  <dt className="text-[11px] text-muted-foreground">
                    {t("common.fields.createdAt")}
                  </dt>
                  <dd className="font-mono-tech tabular-nums text-foreground">
                    {formatDateTime(user.created_at)}
                  </dd>
                </div>

                <div className="sm:col-span-2 flex flex-col gap-1 p-3">
                  <dt className="text-[11px] text-muted-foreground">
                    {t("common.fields.updatedAt")}
                  </dt>
                  <dd className="font-mono-tech tabular-nums text-foreground">
                    {formatDateTime(user.updated_at)}
                  </dd>
                </div>
              </dl>
            </section>

            <section className="space-y-2.5">
              <h3 className="text-xs font-semibold text-foreground">
                {t("userManagement.detail.securitySection")}
              </h3>
              <div className="rounded-lg border border-border bg-card divide-y divide-border">
                <div className="flex items-center justify-between gap-4 px-3.5 py-3">
                  <div className="min-w-0 space-y-0.5">
                    <div className="text-xs font-medium text-foreground">
                      {t("userManagement.detail.controls.statusTitle")}
                    </div>
                    <div className="text-[11px] text-muted-foreground leading-relaxed">
                      {isSuperAdminTarget || isSelf
                        ? t(
                            "userManagement.detail.controls.statusProtectedHint",
                          )
                        : t("userManagement.detail.controls.statusHint")}
                    </div>
                  </div>
                  <Switch
                    checked={user.is_active}
                    disabled={isSuperAdminTarget || isSelf || statusLoading}
                    onCheckedChange={(checked) =>
                      void handleStatusChange(checked)
                    }
                    aria-label={t("userManagement.detail.controls.statusTitle")}
                  />
                </div>

                <div className="flex items-center justify-between gap-4 px-3.5 py-3">
                  <div className="min-w-0 space-y-0.5">
                    <div className="text-xs font-medium text-foreground">
                      {t("userManagement.detail.controls.passwordTitle")}
                    </div>
                    <div className="text-[11px] text-muted-foreground leading-relaxed">
                      {isSuperAdminTarget
                        ? t(
                            "userManagement.detail.controls.passwordProtectedHint",
                          )
                        : t("userManagement.detail.controls.passwordHint")}
                    </div>
                  </div>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={isSuperAdminTarget}
                    onClick={() => setResetPwOpen(true)}
                  >
                    <KeyRound className="h-3.5 w-3.5" />
                    {t("userManagement.detail.resetPassword")}
                  </Button>
                </div>

                <div className="flex items-center justify-between gap-4 px-3.5 py-3">
                  <div className="min-w-0 space-y-0.5">
                    <div className="text-xs font-medium text-foreground">
                      {t("userManagement.detail.controls.mfaTitle")}
                    </div>
                    <div className="text-[11px] text-muted-foreground leading-relaxed">
                      {mfaControlHint}
                    </div>
                  </div>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    loading={mfaLoading}
                    disabled={isSuperAdminTarget || !user.mfa_enabled}
                    onClick={() => setConfirmMfaOpen(true)}
                  >
                    <ShieldOff className="h-3.5 w-3.5" />
                    {t("userManagement.detail.resetMfa")}
                  </Button>
                </div>

                {!isSuperAdminTarget && !isSelf && (
                  <div className="flex items-center justify-between gap-4 px-3.5 py-3">
                    <div className="min-w-0 space-y-0.5">
                      <div className="text-xs font-medium text-danger-fg">
                        {t("userManagement.detail.controls.dangerTitle")}
                      </div>
                      <div className="text-[11px] text-muted-foreground leading-relaxed">
                        {t("userManagement.detail.controls.dangerHint")}
                      </div>
                    </div>
                    <Button
                      type="button"
                      variant="danger"
                      size="sm"
                      loading={deleteLoading}
                      onClick={() => setConfirmDeleteOpen(true)}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                      {t("userManagement.detail.deleteUser")}
                    </Button>
                  </div>
                )}
              </div>
            </section>
          </div>
        ) : null}
      </Drawer>

      <UserEditModal
        open={editOpen}
        admin={user}
        onClose={() => setEditOpen(false)}
        onSuccess={(updated) => {
          setUser(updated);
          onChanged();
        }}
      />

      <UserResetPasswordModal
        open={resetPwOpen}
        admin={user}
        onClose={() => setResetPwOpen(false)}
        onSuccess={() => {
          if (user) {
            void loadUser(user.id);
          }
          onChanged();
        }}
      />

      <ConfirmDialog
        open={confirmMfaOpen}
        onClose={() => setConfirmMfaOpen(false)}
        onConfirm={() => void handleConfirmResetMfa()}
        title={t("userManagement.messages.resetMfaTitle")}
        description={t("userManagement.messages.resetMfaConfirm", {
          username: user?.username ?? "",
        })}
        confirmLabel={t("common.confirm")}
        cancelLabel={t("common.cancel")}
        danger
        loading={mfaLoading}
      />

      <ConfirmDialog
        open={confirmDeleteOpen}
        onClose={() => setConfirmDeleteOpen(false)}
        onConfirm={() => void handleConfirmDelete()}
        title={t("common.tips")}
        description={t("userManagement.messages.deleteUserConfirm", {
          username: user?.username ?? "",
        })}
        confirmLabel={t("common.confirm")}
        cancelLabel={t("common.cancel")}
        danger
        loading={deleteLoading}
      />
    </>
  );
}
