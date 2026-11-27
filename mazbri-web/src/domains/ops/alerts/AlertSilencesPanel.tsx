import React, { useCallback, useEffect, useMemo, useState } from "react";
import { Info, Plus, RefreshCw } from "lucide-react";
import {
  Button,
  Card,
  ConfirmDialog,
  DataTablePage,
  FormField,
  FormModal,
  Input,
  StatusBadge,
  Switch,
  Textarea,
  Tooltip,
  type DataTableColumn,
} from "@/components/ui";
import {
  createAlertInhibit,
  createAlertSilence,
  deleteAlertInhibit,
  deleteAlertSilence,
  fetchAlertInhibits,
  fetchAlertSilences,
  updateAlertInhibit,
  type AlertInhibit,
  type AlertSilence,
} from "@/api/alerts";
import { useT } from "@/locales";
import { formatDateTime, parseDisplayWallClockMs } from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-dialog-service";
import {
  formatAlertMatchers,
  formatAlertTime,
} from "@/utils/ops/alerts-format";
import { showErrorToast, showSuccessToast } from "@/utils/ui/message-guard";
import { useOpsRole } from "../shared/opsHelpers";

function toDatetimeLocal(wallClock: string): string {
  return wallClock.slice(0, 16).replace(" ", "T");
}

function fromDatetimeLocal(localVal: string): string {
  const normalized = localVal.trim().replace("T", " ");
  if (/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/.test(normalized)) {
    return `${normalized}:00`;
  }
  return normalized;
}

function parseMatchersJson(text: string): Record<string, string> {
  const v = JSON.parse(text || "{}") as unknown;
  if (!v || typeof v !== "object" || Array.isArray(v)) {
    throw new Error("Invalid JSON object");
  }
  return v as Record<string, string>;
}

export function AlertSilencesPanel() {
  const { t } = useT();
  const { isAdmin } = useOpsRole();

  const [loadingSilences, setLoadingSilences] = useState(false);
  const [loadingInhibits, setLoadingInhibits] = useState(false);
  const [silences, setSilences] = useState<AlertSilence[]>([]);
  const [inhibits, setInhibits] = useState<AlertInhibit[]>([]);

  const [silenceModalOpen, setSilenceModalOpen] = useState(false);
  const [silenceMatchers, setSilenceMatchers] = useState('{"product_id":"A"}');
  const [silenceStartsAt, setSilenceStartsAt] = useState("");
  const [silenceEndsAt, setSilenceEndsAt] = useState("");
  const [silenceComment, setSilenceComment] = useState("");
  const [savingSilence, setSavingSilence] = useState(false);

  const [inhibitModalOpen, setInhibitModalOpen] = useState(false);
  const [editingInhibitId, setEditingInhibitId] = useState<string | null>(null);
  const [inhibitName, setInhibitName] = useState("");
  const [inhibitSource, setInhibitSource] = useState('{"category":"upstream"}');
  const [inhibitTarget, setInhibitTarget] = useState('{"category":"business"}');
  const [inhibitEqual, setInhibitEqual] = useState("product_id");
  const [inhibitEnabled, setInhibitEnabled] = useState(true);
  const [savingInhibit, setSavingInhibit] = useState(false);

  const [deleteSilenceTarget, setDeleteSilenceTarget] =
    useState<AlertSilence | null>(null);
  const [deleteInhibitTarget, setDeleteInhibitTarget] =
    useState<AlertInhibit | null>(null);
  const [deleting, setDeleting] = useState(false);

  const reloadSilences = useCallback(async () => {
    setLoadingSilences(true);
    try {
      setSilences((await fetchAlertSilences()) || []);
    } finally {
      setLoadingSilences(false);
    }
  }, []);

  const reloadInhibits = useCallback(async () => {
    setLoadingInhibits(true);
    try {
      setInhibits((await fetchAlertInhibits()) || []);
    } finally {
      setLoadingInhibits(false);
    }
  }, []);

  useEffect(() => {
    void reloadSilences();
    void reloadInhibits();
  }, [reloadSilences, reloadInhibits]);

  const openCreateSilence = () => {
    const now = Date.now();
    const end = now + 10 * 60 * 1000;
    setSilenceMatchers('{"product_id":"A"}');
    setSilenceStartsAt(toDatetimeLocal(formatDateTime(now)));
    setSilenceEndsAt(toDatetimeLocal(formatDateTime(end)));
    setSilenceComment("");
    setSilenceModalOpen(true);
  };

  const openCreateInhibit = () => {
    setEditingInhibitId(null);
    setInhibitName("");
    setInhibitSource('{"category":"upstream"}');
    setInhibitTarget('{"category":"business"}');
    setInhibitEqual("product_id");
    setInhibitEnabled(true);
    setInhibitModalOpen(true);
  };

  const openEditInhibit = (row: AlertInhibit) => {
    setEditingInhibitId(row.id);
    setInhibitName(row.name);
    setInhibitSource(JSON.stringify(row.source_matchers ?? {}));
    setInhibitTarget(JSON.stringify(row.target_matchers ?? {}));
    setInhibitEqual((row.equal_labels ?? []).join(", "));
    setInhibitEnabled(row.enabled);
    setInhibitModalOpen(true);
  };

  const handleSaveSilence = async () => {
    setSavingSilence(true);
    try {
      const startMs = parseDisplayWallClockMs(fromDatetimeLocal(silenceStartsAt));
      const endMs = parseDisplayWallClockMs(fromDatetimeLocal(silenceEndsAt));
      if (!Number.isFinite(startMs) || !Number.isFinite(endMs) || endMs <= startMs) {
        throw new Error(t("ops.alerts.silences.window"));
      }
      await createAlertSilence({
        matchers: parseMatchersJson(silenceMatchers),
        starts_at: new Date(startMs).toISOString(),
        ends_at: new Date(endMs).toISOString(),
        comment: silenceComment.trim(),
      });
      setSilenceModalOpen(false);
      showSuccessToast(t("common.success"));
      await reloadSilences();
    } catch (error) {
      if (isMfaStepUpCancelled(error)) return;
      showErrorToast(
        error instanceof Error ? error.message : t("common.failed"),
      );
    } finally {
      setSavingSilence(false);
    }
  };

  const handleSaveInhibit = async () => {
    setSavingInhibit(true);
    try {
      const payload = {
        name: inhibitName.trim(),
        source_matchers: parseMatchersJson(inhibitSource),
        target_matchers: parseMatchersJson(inhibitTarget),
        equal_labels: inhibitEqual
          .split(",")
          .map((s) => s.trim())
          .filter(Boolean),
        enabled: inhibitEnabled,
      };
      if (editingInhibitId) {
        await updateAlertInhibit(editingInhibitId, payload);
      } else {
        await createAlertInhibit(payload);
      }
      setInhibitModalOpen(false);
      showSuccessToast(t("common.success"));
      await reloadInhibits();
    } catch (error) {
      if (isMfaStepUpCancelled(error)) return;
      showErrorToast(
        error instanceof Error ? error.message : t("common.failed"),
      );
    } finally {
      setSavingInhibit(false);
    }
  };

  const handleToggleInhibit = async (row: AlertInhibit, nextEnabled: boolean) => {
    try {
      await updateAlertInhibit(row.id, {
        name: row.name,
        source_matchers: row.source_matchers,
        target_matchers: row.target_matchers,
        equal_labels: row.equal_labels,
        enabled: nextEnabled,
      });
      showSuccessToast(t("common.success"));
      await reloadInhibits();
    } catch (error) {
      if (isMfaStepUpCancelled(error)) return;
    }
  };

  const confirmDeleteSilence = async () => {
    if (!deleteSilenceTarget) return;
    setDeleting(true);
    try {
      await deleteAlertSilence(deleteSilenceTarget.id);
      setDeleteSilenceTarget(null);
      await reloadSilences();
    } catch (error) {
      if (isMfaStepUpCancelled(error)) return;
    } finally {
      setDeleting(false);
    }
  };

  const confirmDeleteInhibit = async () => {
    if (!deleteInhibitTarget) return;
    setDeleting(true);
    try {
      await deleteAlertInhibit(deleteInhibitTarget.id);
      setDeleteInhibitTarget(null);
      await reloadInhibits();
    } catch (error) {
      if (isMfaStepUpCancelled(error)) return;
    } finally {
      setDeleting(false);
    }
  };

  const silenceColumns = useMemo<DataTableColumn<AlertSilence>[]>(() => {
    const cols: DataTableColumn<AlertSilence>[] = [
      {
        id: "matchers",
        header: t("ops.alerts.silences.matchers"),
        minWidth: 200,
        mono: true,
        cell: (row) => formatAlertMatchers(row.matchers, t("common.all")),
      },
      {
        id: "window",
        header: t("ops.alerts.silences.window"),
        minWidth: 280,
        mono: true,
        cell: (row) =>
          `${formatAlertTime(row.starts_at)} → ${formatAlertTime(row.ends_at)}`,
      },
      {
        id: "created_by",
        header: t("ops.alerts.silences.createdBy"),
        width: 120,
        accessorKey: "created_by",
      },
      {
        id: "comment",
        header: t("ops.alerts.silences.comment"),
        minWidth: 150,
        accessorKey: "comment",
      },
    ];
    if (isAdmin) {
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
            className="text-danger-fg hover:text-danger-fg"
            onClick={() => setDeleteSilenceTarget(row)}
          >
            {t("common.delete")}
          </Button>
        ),
      });
    }
    return cols;
  }, [t, isAdmin]);

  const inhibitColumns = useMemo<DataTableColumn<AlertInhibit>[]>(() => {
    const cols: DataTableColumn<AlertInhibit>[] = [
      {
        id: "name",
        header: t("ops.alerts.inhibits.name"),
        minWidth: 140,
        cell: (row) => (
          <span className="font-medium text-foreground">{row.name}</span>
        ),
      },
      {
        id: "source",
        header: t("ops.alerts.inhibits.source"),
        minWidth: 160,
        mono: true,
        cell: (row) =>
          formatAlertMatchers(row.source_matchers, t("common.all")),
      },
      {
        id: "target",
        header: t("ops.alerts.inhibits.target"),
        minWidth: 160,
        mono: true,
        cell: (row) =>
          formatAlertMatchers(row.target_matchers, t("common.all")),
      },
      {
        id: "equal",
        header: t("ops.alerts.inhibits.equal"),
        minWidth: 120,
        mono: true,
        cell: (row) => row.equal_labels?.join(", ") || "—",
      },
      {
        id: "enabled",
        header: t("ops.alerts.rules.columns.enabled"),
        width: 96,
        cell: (row) =>
          isAdmin ? (
            <Switch
              checked={row.enabled}
              aria-label={row.name}
              onCheckedChange={(next) => void handleToggleInhibit(row, next)}
            />
          ) : (
            <StatusBadge status={row.enabled ? "ENABLED" : "DISABLED"} />
          ),
      },
    ];
    if (isAdmin) {
      cols.push({
        id: "operations",
        header: t("common.fields.operations"),
        width: 120,
        align: "right",
        cell: (row) => (
          <div className="flex items-center justify-end gap-1">
            <Button
              type="button"
              variant="link"
              size="xs"
              onClick={() => openEditInhibit(row)}
            >
              {t("common.actions.edit")}
            </Button>
            <Button
              type="button"
              variant="link"
              size="xs"
              className="text-danger-fg hover:text-danger-fg"
              onClick={() => setDeleteInhibitTarget(row)}
            >
              {t("common.delete")}
            </Button>
          </div>
        ),
      });
    }
    return cols;
  }, [t, isAdmin]);

  return (
    <div className="flex flex-col flex-1 min-h-0 gap-4 min-w-0">
      {/* Silences Card: unified Card Header (fixes A-09 external floating Refresh) */}
      <Card className="flex flex-col flex-1 min-h-0 overflow-hidden">
        <div className="flex flex-wrap items-center justify-between gap-2.5 border-b border-border px-4 py-3 shrink-0">
          <div className="flex items-center gap-1.5 min-w-0">
            <h3 className="text-xs font-semibold text-foreground">
              {t("menus.mazbri.alertSilences")}
            </h3>
            <Tooltip content={t("ops.alerts.silences.hint")}>
              <span className="inline-flex text-muted-foreground hover:text-foreground cursor-help">
                <Info className="h-3.5 w-3.5" />
              </span>
            </Tooltip>
          </div>

          <div className="flex items-center gap-2 shrink-0">
            <Button
              type="button"
              variant="ghost"
              size="md"
              loading={loadingSilences}
              onClick={() => void reloadSilences()}
            >
              <RefreshCw className="h-3.5 w-3.5" />
              <span>{t("common.refresh")}</span>
            </Button>
            {isAdmin && (
              <Button
                type="button"
                variant="primary"
                size="md"
                onClick={openCreateSilence}
              >
                <Plus className="h-3.5 w-3.5" />
                <span>{t("ops.alerts.silences.create")}</span>
              </Button>
            )}
          </div>
        </div>

        <DataTablePage
          columns={silenceColumns}
          data={silences}
          getRowId={(r) => r.id}
          loading={loadingSilences}
          emptyText={t("ops.charts.empty")}
          className="[&>div]:border-0 [&>div]:rounded-none"
        />
      </Card>

      {/* Inhibits Card: unified Card Header */}
      <Card className="flex flex-col flex-1 min-h-0 overflow-hidden">
        <div className="flex flex-wrap items-center justify-between gap-2.5 border-b border-border px-4 py-3 shrink-0">
          <h3 className="text-xs font-semibold text-foreground">
            {t("ops.alerts.inhibits.title")}
          </h3>

          <div className="flex items-center gap-2 shrink-0">
            <Button
              type="button"
              variant="ghost"
              size="md"
              loading={loadingInhibits}
              onClick={() => void reloadInhibits()}
            >
              <RefreshCw className="h-3.5 w-3.5" />
              <span>{t("common.refresh")}</span>
            </Button>
            {isAdmin && (
              <Button
                type="button"
                variant="primary"
                size="md"
                onClick={openCreateInhibit}
              >
                <Plus className="h-3.5 w-3.5" />
                <span>{t("ops.alerts.inhibits.create")}</span>
              </Button>
            )}
          </div>
        </div>

        <DataTablePage
          columns={inhibitColumns}
          data={inhibits}
          getRowId={(r) => r.id}
          loading={loadingInhibits}
          emptyText={t("ops.charts.empty")}
          className="[&>div]:border-0 [&>div]:rounded-none"
        />
      </Card>

      {/* Create Silence Modal (strict UTC+8 wall-clock binding - U2) */}
      <FormModal
        open={silenceModalOpen}
        onClose={() => setSilenceModalOpen(false)}
        onSubmit={handleSaveSilence}
        title={t("ops.alerts.silences.create")}
        size="md"
        submitting={savingSilence}
      >
        <FormField label={t("ops.alerts.silences.matchers")} required>
          <Textarea
            rows={3}
            value={silenceMatchers}
            onChange={(e) => setSilenceMatchers(e.target.value)}
            className="font-mono-tech"
            placeholder='{"product_id":"A"}'
          />
        </FormField>

        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <FormField
            label={t("ops.alerts.silences.startsAt")}
            required
          >
            <input
              type="datetime-local"
              value={silenceStartsAt}
              onChange={(e) => setSilenceStartsAt(e.target.value)}
              className="flex h-8 w-full rounded-md border border-input bg-card px-2.5 text-xs font-mono-tech text-foreground hover:border-border-hover focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary/30"
            />
          </FormField>
          <FormField
            label={t("ops.alerts.silences.endsAt")}
            required
          >
            <input
              type="datetime-local"
              value={silenceEndsAt}
              onChange={(e) => setSilenceEndsAt(e.target.value)}
              className="flex h-8 w-full rounded-md border border-input bg-card px-2.5 text-xs font-mono-tech text-foreground hover:border-border-hover focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary/30"
            />
          </FormField>
        </div>

        <FormField label={t("ops.alerts.silences.comment")}>
          <Input
            value={silenceComment}
            onChange={(e) => setSilenceComment(e.target.value)}
          />
        </FormField>
      </FormModal>

      {/* Create / Edit Inhibit Modal */}
      <FormModal
        open={inhibitModalOpen}
        onClose={() => setInhibitModalOpen(false)}
        onSubmit={handleSaveInhibit}
        title={
          editingInhibitId
            ? t("common.actions.edit")
            : t("ops.alerts.inhibits.create")
        }
        size="md"
        submitting={savingInhibit}
      >
        <FormField label={t("ops.alerts.inhibits.name")} required>
          <Input
            value={inhibitName}
            onChange={(e) => setInhibitName(e.target.value)}
          />
        </FormField>

        <FormField label={t("ops.alerts.inhibits.source")} required>
          <Textarea
            rows={2}
            value={inhibitSource}
            onChange={(e) => setInhibitSource(e.target.value)}
            className="font-mono-tech"
            placeholder='{"category":"upstream"}'
          />
        </FormField>

        <FormField label={t("ops.alerts.inhibits.target")} required>
          <Textarea
            rows={2}
            value={inhibitTarget}
            onChange={(e) => setInhibitTarget(e.target.value)}
            className="font-mono-tech"
            placeholder='{"category":"business"}'
          />
        </FormField>

        <FormField label={t("ops.alerts.inhibits.equal")}>
          <Input
            value={inhibitEqual}
            onChange={(e) => setInhibitEqual(e.target.value)}
            className="font-mono-tech"
            placeholder="product_id"
          />
        </FormField>

        <div className="flex items-center justify-between rounded-lg border border-border px-3 py-2">
          <span className="text-xs font-medium text-foreground">
            {t("ops.alerts.rules.columns.enabled")}
          </span>
          <Switch
            checked={inhibitEnabled}
            onCheckedChange={setInhibitEnabled}
          />
        </div>
      </FormModal>

      <ConfirmDialog
        open={Boolean(deleteSilenceTarget)}
        onClose={() => setDeleteSilenceTarget(null)}
        onConfirm={() => void confirmDeleteSilence()}
        title={t("common.delete")}
        description={t("common.confirmDelete")}
        confirmLabel={t("common.delete")}
        danger
        loading={deleting}
      />

      <ConfirmDialog
        open={Boolean(deleteInhibitTarget)}
        onClose={() => setDeleteInhibitTarget(null)}
        onConfirm={() => void confirmDeleteInhibit()}
        title={t("common.delete")}
        description={t("common.confirmDelete")}
        confirmLabel={t("common.delete")}
        danger
        loading={deleting}
      />
    </div>
  );
}
