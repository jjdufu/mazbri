import React, { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router";
import { Info, Plus, RefreshCw, Search } from "lucide-react";
import {
  Badge,
  Button,
  ConfirmDialog,
  DataTablePage,
  FormDrawer,
  FormField,
  Input,
  NumberInput,
  SegmentedControl,
  Select,
  Switch,
  Textarea,
  Tooltip,
  type DataTableColumn,
  type SelectOptionItem,
} from "@/components/ui";
import {
  createAlertRule,
  deleteAlertRule,
  fetchAlertRules,
  setAlertRuleEnabled,
  updateAlertRule,
} from "@/api/alerts";
import { useT } from "@/locales";
import type {
  AlertOp,
  AlertRule,
  AlertRuleInput,
  AlertRuleKind,
  AlertSeverity,
} from "@/types/api/alerts";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-dialog-service";
import {
  formatAlertLabelsText,
  formatRuleCondition,
  isValidAlertEmail,
  parseAlertLabelsText,
  parseAlertListText,
} from "@/utils/ops/alerts-format";
import {
  buildAlertsRoute,
  resolveAlertDrillRoute,
} from "@/utils/ops/alerts-link";
import { showSuccessToast } from "@/utils/ui/message-guard";
import { toLocationHref, useOpsRole } from "../shared/opsHelpers";

const ALERT_OPS: readonly AlertOp[] = [">", ">=", "<", "<=", "==", "!="];
const ALERT_SEVERITIES: readonly AlertSeverity[] = [
  "critical",
  "warning",
  "info",
];
const OLAP_PLACEHOLDER =
  '{"query":{"template":"top_by_dim","params":{"dim":"product_id","metric":"p99"}},"value_field":"value"}';

interface RuleFormState {
  name: string;
  kind: AlertRuleKind;
  expr: string;
  threshold: number;
  op: AlertOp;
  for_seconds: number;
  severity: AlertSeverity;
  labelsText: string;
  notifyEnabled: boolean;
  emailsText: string;
  webhooksText: string;
  enabled: boolean;
}

const emptyForm = (): RuleFormState => ({
  name: "",
  kind: "promql",
  expr: "",
  threshold: 0,
  op: ">",
  for_seconds: 0,
  severity: "warning",
  labelsText: "",
  notifyEnabled: false,
  emailsText: "",
  webhooksText: "",
  enabled: true,
});

function severityBadgeVariant(
  severity: AlertSeverity,
): "danger" | "warning" | "neutral" {
  if (severity === "critical") return "danger";
  if (severity === "warning") return "warning";
  return "neutral";
}

function LabelWithHint({ label, hint }: { label: string; hint: string }) {
  return (
    <span className="inline-flex items-center gap-1">
      <span>{label}</span>
      <Tooltip content={hint}>
        <span className="inline-flex text-muted-foreground hover:text-foreground cursor-help">
          <Info className="h-3.5 w-3.5" />
        </span>
      </Tooltip>
    </span>
  );
}

export function AlertRulesPanel() {
  const { t, locale } = useT();
  const navigate = useNavigate();
  const { isAdmin, isBiz, isViewer } = useOpsRole();

  const [loading, setLoading] = useState(false);
  const [rules, setRules] = useState<AlertRule[]>([]);
  const [keyword, setKeyword] = useState("");
  const [severityFilter, setSeverityFilter] = useState<AlertSeverity | "">("");
  const [kindFilter, setKindFilter] = useState<AlertRuleKind | "">("");
  const [enabledFilter, setEnabledFilter] = useState<"" | "enabled" | "disabled">(
    "",
  );

  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [form, setForm] = useState<RuleFormState>(emptyForm);
  const [errors, setErrors] = useState<Partial<Record<keyof RuleFormState, string>>>(
    {},
  );
  const [saving, setSaving] = useState(false);

  const [deleteTarget, setDeleteTarget] = useState<AlertRule | null>(null);
  const [deleting, setDeleting] = useState(false);

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      const list = await fetchAlertRules();
      setRules(list ?? []);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const filteredRules = useMemo(() => {
    const q = keyword.trim().toLowerCase();
    return rules.filter((r) => {
      if (severityFilter && r.severity !== severityFilter) return false;
      if (kindFilter && r.kind !== kindFilter) return false;
      if (enabledFilter === "enabled" && !r.enabled) return false;
      if (enabledFilter === "disabled" && r.enabled) return false;
      if (
        q &&
        !r.name.toLowerCase().includes(q) &&
        !r.expr.toLowerCase().includes(q)
      ) {
        return false;
      }
      return true;
    });
  }, [rules, keyword, severityFilter, kindFilter, enabledFilter]);

  const drillRouteOf = useCallback(
    (rule: AlertRule) =>
      resolveAlertDrillRoute(
        { labels: rule.labels, expr: rule.expr },
        { isAdmin, isBiz, isViewer },
      ),
    [isAdmin, isBiz, isViewer],
  );

  const openCreate = () => {
    setEditingId(null);
    setForm(emptyForm());
    setErrors({});
    setDrawerOpen(true);
  };

  const openEdit = (rule: AlertRule) => {
    setEditingId(rule.id);
    setForm({
      name: rule.name,
      kind: rule.kind,
      expr: rule.expr,
      threshold: rule.threshold,
      op: rule.op,
      for_seconds: rule.for_seconds,
      severity: rule.severity,
      labelsText: formatAlertLabelsText(rule.labels),
      notifyEnabled: rule.notify?.enabled ?? false,
      emailsText: (rule.notify?.emails || []).join("\n"),
      webhooksText: (rule.notify?.webhooks || []).join("\n"),
      enabled: rule.enabled,
    });
    setErrors({});
    setDrawerOpen(true);
  };

  const validateForm = (): boolean => {
    const nextErrors: Partial<Record<keyof RuleFormState, string>> = {};
    if (!form.name.trim()) {
      nextErrors.name = t("ops.alerts.rules.form.required");
    }
    if (!form.expr.trim()) {
      nextErrors.expr = t("ops.alerts.rules.form.required");
    }
    if (!parseAlertLabelsText(form.labelsText)) {
      nextErrors.labelsText = t("ops.alerts.rules.form.labelsInvalid");
    }
    const emails = parseAlertListText(form.emailsText);
    const webhooks = parseAlertListText(form.webhooksText);
    if (!emails.every((email) => isValidAlertEmail(email))) {
      nextErrors.emailsText = t("ops.alerts.rules.form.emailsInvalid");
    } else if (form.notifyEnabled && emails.length === 0 && webhooks.length === 0) {
      nextErrors.emailsText = t("ops.alerts.rules.form.notifyRequired");
    }
    setErrors(nextErrors);
    return Object.keys(nextErrors).length === 0;
  };

  const handleSave = async () => {
    if (!validateForm()) return;
    const payload: AlertRuleInput = {
      name: form.name.trim(),
      kind: form.kind,
      expr: form.expr.trim(),
      threshold: form.threshold,
      op: form.op,
      for_seconds: form.for_seconds,
      severity: form.severity,
      labels: parseAlertLabelsText(form.labelsText) || {},
      notify: {
        enabled: form.notifyEnabled,
        emails: parseAlertListText(form.emailsText),
        webhooks: parseAlertListText(form.webhooksText),
      },
      enabled: form.enabled,
    };
    setSaving(true);
    try {
      if (editingId) {
        await updateAlertRule(editingId, payload);
      } else {
        await createAlertRule(payload);
      }
      showSuccessToast(t("ops.alerts.rules.saved"));
      setDrawerOpen(false);
      await reload();
    } catch (error) {
      if (isMfaStepUpCancelled(error)) return;
    } finally {
      setSaving(false);
    }
  };

  const handleToggle = async (rule: AlertRule, enabled: boolean) => {
    try {
      await setAlertRuleEnabled(rule.id, enabled);
      showSuccessToast(t("ops.alerts.rules.saved"));
      await reload();
    } catch (error) {
      if (isMfaStepUpCancelled(error)) return;
    }
  };

  const handleConfirmDelete = async () => {
    if (!deleteTarget) return;
    setDeleting(true);
    try {
      await deleteAlertRule(deleteTarget.id);
      showSuccessToast(t("ops.alerts.rules.deleted"));
      setDeleteTarget(null);
      await reload();
    } catch (error) {
      if (isMfaStepUpCancelled(error)) return;
    } finally {
      setDeleting(false);
    }
  };

  const severityFilterOptions = useMemo<SelectOptionItem[]>(
    () => [
      {
        value: "",
        label: `${t("ops.alerts.filters.severity")}: ${t("common.all")}`,
        text: t("common.all"),
      },
      ...ALERT_SEVERITIES.map((s) => ({
        value: s,
        label: t(`ops.alerts.severity.${s}`),
        text: t(`ops.alerts.severity.${s}`),
      })),
    ],
    [t],
  );

  const kindFilterOptions = useMemo<SelectOptionItem[]>(
    () => [
      {
        value: "",
        label: `${t("ops.alerts.rules.form.kind")}: ${t("common.all")}`,
        text: t("common.all"),
      },
      { value: "promql", label: "PromQL", text: "PromQL" },
      { value: "olap", label: "OLAP", text: "OLAP" },
    ],
    [t],
  );

  const enabledFilterOptions = useMemo<SelectOptionItem[]>(
    () => [
      {
        value: "",
        label: `${t("ops.alerts.columns.status")}: ${t("common.all")}`,
        text: t("common.all"),
      },
      {
        value: "enabled",
        label: t("common.status.enabled"),
        text: t("common.status.enabled"),
      },
      {
        value: "disabled",
        label: t("common.status.disabled"),
        text: t("common.status.disabled"),
      },
    ],
    [t],
  );

  const columns = useMemo<DataTableColumn<AlertRule>[]>(() => {
    const cols: DataTableColumn<AlertRule>[] = [
      {
        id: "enabled",
        header: t("ops.alerts.rules.columns.enabled"),
        width: 68,
        cell: (row) => (
          <Switch
            checked={row.enabled}
            disabled={!isAdmin}
            aria-label={row.name}
            onCheckedChange={(next) => void handleToggle(row, next)}
          />
        ),
      },
      {
        id: "name",
        header: t("ops.alerts.rules.columns.name"),
        minWidth: 150,
        cell: (row) => (
          <span className="font-medium text-foreground">{row.name}</span>
        ),
      },
      {
        id: "severity",
        header: t("ops.alerts.rules.columns.severity"),
        width: 80,
        cell: (row) => (
          <Badge variant={severityBadgeVariant(row.severity)}>
            {t(`ops.alerts.severity.${row.severity}`)}
          </Badge>
        ),
      },
      {
        id: "expr",
        header: t("ops.alerts.rules.columns.expr"),
        minWidth: 200,
        cell: (row) => (
          <code
            title={row.expr}
            className="block max-w-[280px] truncate rounded-md bg-muted/60 px-1.5 py-0.5 font-mono-tech text-[11px] text-foreground"
          >
            {row.kind} · {row.expr}
          </code>
        ),
      },
      {
        id: "condition",
        header: t("ops.alerts.rules.columns.condition"),
        minWidth: 164,
        cell: (row) => (
          <span className="font-mono-tech tabular-nums">
            {formatRuleCondition(row, t, locale)}
          </span>
        ),
      },
      {
        id: "notify",
        header: t("ops.alerts.rules.columns.notify"),
        width: 140,
        cell: (row) =>
          row.notify?.enabled ? (
            <span>
              {t("ops.alerts.rules.notifySummary", {
                emails: row.notify.emails?.length || 0,
                webhooks: row.notify.webhooks?.length || 0,
              })}
            </span>
          ) : (
            <span className="text-muted-foreground">
              {t("ops.alerts.rules.notifyOff")}
            </span>
          ),
      },
      {
        id: "active_events",
        header: t("ops.alerts.rules.columns.active"),
        width: 80,
        align: "right",
        cell: (row) =>
          row.active_events > 0 ? (
            <Button
              type="button"
              variant="link"
              size="xs"
              className="text-danger-fg hover:text-danger-fg font-mono-tech"
              onClick={() =>
                void navigate(toLocationHref(buildAlertsRoute({ tab: "events" })))
              }
            >
              {row.active_events}
            </Button>
          ) : (
            <span className="font-mono-tech tabular-nums text-muted-foreground">
              0
            </span>
          ),
      },
    ];

    if (isAdmin) {
      cols.push({
        id: "actions",
        header: t("ops.alerts.columns.actions"),
        width: 170,
        align: "right",
        cell: (row) => {
          const drillTarget = drillRouteOf(row);
          return (
            <div className="flex items-center justify-end gap-1">
              {drillTarget && (
                <Button
                  type="button"
                  variant="link"
                  size="xs"
                  onClick={() => void navigate(toLocationHref(drillTarget))}
                >
                  {t("ops.alerts.drillDown")}
                </Button>
              )}
              <Button
                type="button"
                variant="link"
                size="xs"
                onClick={() => openEdit(row)}
              >
                {t("ops.alerts.rules.edit")}
              </Button>
              <Button
                type="button"
                variant="link"
                size="xs"
                className="text-danger-fg hover:text-danger-fg"
                onClick={() => setDeleteTarget(row)}
              >
                {t("ops.alerts.rules.delete")}
              </Button>
            </div>
          );
        },
      });
    }
    return cols;
  }, [t, locale, isAdmin, drillRouteOf, navigate]);

  return (
    <div className="flex flex-col flex-1 min-h-0 gap-4 min-w-0">
      <DataTablePage
        columns={columns}
        data={filteredRules}
        getRowId={(r) => r.id}
        loading={loading}
        emptyText={t("ops.charts.empty")}
        filters={
          <>
            <div className="relative flex items-center w-56">
              <Search className="pointer-events-none absolute left-2.5 h-3.5 w-3.5 text-muted-foreground" />
              <Input
                value={keyword}
                onChange={(e) => setKeyword(e.target.value)}
                placeholder={t("common.search")}
                className="pl-8"
              />
            </div>

            <Select
              value={severityFilter}
              options={severityFilterOptions}
              placeholder={t("ops.alerts.filters.severity")}
              onChange={(e) =>
                setSeverityFilter(e.target.value as AlertSeverity | "")
              }
              className="w-36"
            />

            <Select
              value={kindFilter}
              options={kindFilterOptions}
              placeholder={t("ops.alerts.rules.form.kind")}
              onChange={(e) =>
                setKindFilter(e.target.value as AlertRuleKind | "")
              }
              className="w-36"
            />

            <Select
              value={enabledFilter}
              options={enabledFilterOptions}
              placeholder={t("ops.alerts.columns.status")}
              onChange={(e) =>
                setEnabledFilter(
                  e.target.value as "" | "enabled" | "disabled",
                )
              }
              className="w-36"
            />
          </>
        }
        actions={
          <>
            <Button
              type="button"
              variant="ghost"
              size="md"
              loading={loading}
              onClick={() => void reload()}
            >
              <RefreshCw className="h-3.5 w-3.5" />
              <span>{t("ops.alerts.rules.refresh")}</span>
            </Button>
            {isAdmin && (
              <Button
                type="button"
                variant="primary"
                size="md"
                onClick={openCreate}
              >
                <Plus className="h-3.5 w-3.5" />
                <span>{t("ops.alerts.rules.create")}</span>
              </Button>
            )}
          </>
        }
      />

      {/* Right-side three-part FormDrawer (600px, label-top, aligned condition grid - A-14) */}
      <FormDrawer
        open={drawerOpen}
        onClose={() => setDrawerOpen(false)}
        onSubmit={handleSave}
        title={
          editingId ? t("ops.alerts.rules.edit") : t("ops.alerts.rules.create")
        }
        size="md"
        submitting={saving}
        submitLabel={t("ops.alerts.rules.save")}
        cancelLabel={t("ops.alerts.rules.cancel")}
      >
        <FormField
          label={t("ops.alerts.rules.columns.name")}
          required
          error={errors.name}
        >
          <Input
            maxLength={128}
            value={form.name}
            onChange={(e) => setForm((f) => ({ ...f, name: e.target.value }))}
          />
        </FormField>

        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <FormField label={t("ops.alerts.rules.form.kind")} required>
            <SegmentedControl
              value={form.kind}
              onChange={(kind) => setForm((f) => ({ ...f, kind }))}
              items={[
                { key: "promql", label: "PromQL" },
                { key: "olap", label: "OLAP" },
              ]}
            />
          </FormField>

          <FormField label={t("ops.alerts.rules.columns.severity")} required>
            <Select
              value={form.severity}
              options={ALERT_SEVERITIES.map((s) => ({
                value: s,
                label: t(`ops.alerts.severity.${s}`),
                text: t(`ops.alerts.severity.${s}`),
              }))}
              onChange={(e) =>
                setForm((f) => ({
                  ...f,
                  severity: e.target.value as AlertSeverity,
                }))
              }
              className="w-full"
            />
          </FormField>
        </div>

        <FormField
          label={
            <LabelWithHint
              label={t("ops.alerts.rules.columns.expr")}
              hint={t("ops.alerts.rules.form.exprHint")}
            />
          }
          required
          error={errors.expr}
        >
          <Textarea
            rows={3}
            maxLength={4000}
            value={form.expr}
            onChange={(e) => setForm((f) => ({ ...f, expr: e.target.value }))}
            className="font-mono-tech"
            placeholder={
              form.kind === "olap"
                ? OLAP_PLACEHOLDER
                : "max by (name) (circuit_breaker_state)"
            }
          />
        </FormField>

        {/* Non-wrapping horizontal grid for condition row */}
        <FormField label={t("ops.alerts.rules.columns.condition")} required>
          <div className="grid grid-cols-[80px_minmax(0,1fr)_auto_124px_auto] items-center gap-2">
            <Select
              value={form.op}
              options={ALERT_OPS.map((op) => ({
                value: op,
                label: op,
                text: op,
              }))}
              onChange={(e) =>
                setForm((f) => ({ ...f, op: e.target.value as AlertOp }))
              }
              className="w-full font-mono-tech"
            />
            <NumberInput
              value={form.threshold}
              step={1}
              onChange={(threshold) => setForm((f) => ({ ...f, threshold }))}
              aria-label={t("ops.alerts.rules.columns.condition")}
            />
            <span className="text-xs text-muted-foreground whitespace-nowrap">
              {t("ops.alerts.rules.form.for")}
            </span>
            <NumberInput
              value={form.for_seconds}
              min={0}
              max={86400}
              step={30}
              onChange={(for_seconds) =>
                setForm((f) => ({ ...f, for_seconds }))
              }
              aria-label={t("ops.alerts.rules.form.for")}
            />
            <span className="text-xs text-muted-foreground whitespace-nowrap">
              {t("ops.units.seconds")}
            </span>
          </div>
        </FormField>

        <FormField
          label={
            <LabelWithHint
              label={t("ops.alerts.rules.form.labels")}
              hint={t("ops.alerts.rules.form.labelsInvalid")}
            />
          }
          error={errors.labelsText}
        >
          <Textarea
            rows={2}
            value={form.labelsText}
            onChange={(e) =>
              setForm((f) => ({ ...f, labelsText: e.target.value }))
            }
            className="font-mono-tech"
            placeholder="category=upstream"
          />
        </FormField>

        <FormField
          label={
            <LabelWithHint
              label={t("ops.alerts.rules.form.emails")}
              hint={t("ops.alerts.rules.form.emailsHint")}
            />
          }
          error={errors.emailsText}
        >
          <Textarea
            rows={2}
            value={form.emailsText}
            onChange={(e) =>
              setForm((f) => ({ ...f, emailsText: e.target.value }))
            }
            className="font-mono-tech"
            placeholder="oncall@example.com"
          />
        </FormField>

        <FormField
          label={
            <LabelWithHint
              label={t("ops.alerts.rules.form.webhooks")}
              hint={t("ops.alerts.rules.form.webhooksHint")}
            />
          }
        >
          <Textarea
            rows={2}
            value={form.webhooksText}
            onChange={(e) =>
              setForm((f) => ({ ...f, webhooksText: e.target.value }))
            }
            className="font-mono-tech"
            placeholder="https://hooks.example.com/alert"
          />
        </FormField>

        <div className="grid grid-cols-1 gap-3 rounded-lg border border-border p-3 sm:grid-cols-2">
          <div className="flex items-center justify-between gap-2">
            <LabelWithHint
              label={t("ops.alerts.rules.form.notifyEnabled")}
              hint={t("ops.alerts.rules.form.notifyEnabledHint")}
            />
            <Switch
              checked={form.notifyEnabled}
              onCheckedChange={(notifyEnabled) =>
                setForm((f) => ({ ...f, notifyEnabled }))
              }
            />
          </div>
          <div className="flex items-center justify-between gap-2">
            <LabelWithHint
              label={t("ops.alerts.rules.form.evaluateEnabled")}
              hint={t("ops.alerts.rules.form.evaluateEnabledHint")}
            />
            <Switch
              checked={form.enabled}
              onCheckedChange={(enabled) => setForm((f) => ({ ...f, enabled }))}
            />
          </div>
        </div>
      </FormDrawer>

      <ConfirmDialog
        open={Boolean(deleteTarget)}
        onClose={() => setDeleteTarget(null)}
        onConfirm={() => void handleConfirmDelete()}
        title={t("ops.alerts.rules.delete")}
        description={
          deleteTarget
            ? t("ops.alerts.rules.deleteConfirm", { name: deleteTarget.name })
            : ""
        }
        confirmLabel={t("ops.alerts.rules.delete")}
        danger
        loading={deleting}
      />
    </div>
  );
}
