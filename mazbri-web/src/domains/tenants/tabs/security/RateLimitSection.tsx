import { useCallback, useEffect, useMemo, useState } from "react";
import { Link } from "react-router";
import {
  AlertTriangle,
  ChevronDown,
  ChevronRight,
  Gauge,
  Info,
  Plus,
  RefreshCw,
  Save,
  Trash2,
} from "lucide-react";
import {
  cancelTenantRateLimitCommand,
  createTenantRateLimitCommand,
  fetchTenantRateLimitCommands,
  fetchTenantRateLimitConfig,
  updateTenantRateLimitConfig,
} from "@/api/tenant";
import {
  ActionChip,
  Badge,
  Button,
  ConfirmDialog,
  EntityDetailCard,
  IconButton,
  Input,
  Modal,
  NumberInput,
  Switch,
  Tooltip,
  cn,
} from "@/components/ui";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import type {
  RateLimitControlCommand,
  TenantRateLimitConfig,
  UpdateTenantRateLimitConfigRequest,
} from "@/types/api/tenant";
import { HttpError } from "@/utils/http/error";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { isBizRole } from "@/utils/ops/roles";
import {
  PLAN_RATE_TEMPLATES,
  RATE_LIMIT_REASON_GUIDE,
  reasonNavLocation,
  type PlanRateTemplate,
} from "@/utils/rate-limit-reason";
import { logger } from "@/utils/sys/logger";
import {
  showSuccessToast,
  showWarningToast,
} from "@/utils/ui/message-guard";
import { summarizeRateLimitConflict } from "../../helpers/conflict-summary";
import { commandTypeLabel, formatTime } from "../../helpers/rate-limit-helpers";

export interface RateLimitSectionProps {
  tenantCode: string;
}

type PendingOpsAction =
  | { type: "deny" }
  | { type: "quota" }
  | { type: "refresh" }
  | { type: "cancel"; row: RateLimitControlCommand };

export function RateLimitSection({ tenantCode }: RateLimitSectionProps) {
  const { t } = useT();
  const role = useAuthStore((s) => s.info.role);
  const canWrite = isBizRole(role);

  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [commandsLoading, setCommandsLoading] = useState(false);
  const [opsBusy, setOpsBusy] = useState(false);

  const [enabled, setEnabled] = useState(true);
  const [maxConcurrent, setMaxConcurrent] = useState(1000);
  const [burstSize, setBurstSize] = useState(100);
  const [clientIpQps, setClientIpQps] = useState(1000);
  const [clientIpBurst, setClientIpBurst] = useState(100);
  const [maxBandwidthMbps, setMaxBandwidthMbps] = useState(0);
  const [expectedUpdatedAt, setExpectedUpdatedAt] = useState<
    string | undefined
  >(undefined);
  const [clientIpManuallyEdited, setClientIpManuallyEdited] = useState(false);

  const [apiLimits, setApiLimits] = useState<
    Array<{ path: string; qps: number; burst: number }>
  >([]);

  const [commands, setCommands] = useState<RateLimitControlCommand[]>([]);
  const [denyMinutes, setDenyMinutes] = useState(5);
  const [quotaMultiplier, setQuotaMultiplier] = useState(0.5);
  const [quotaExpiresMinutes, setQuotaExpiresMinutes] = useState(60);
  const [commandReason, setCommandReason] = useState("");

  const [apiOverridesExpanded, setApiOverridesExpanded] = useState(false);
  const [opsExpanded, setOpsExpanded] = useState(false);
  const [reasonGuideExpanded, setReasonGuideExpanded] = useState(false);

  const [conflictText, setConflictText] = useState<string | null>(null);
  const [pendingOps, setPendingOps] = useState<PendingOpsAction | null>(null);

  const applyConfig = useCallback((config: TenantRateLimitConfig) => {
    const tenantQps = config.max_concurrent;
    const burst = config.burst_size;
    const ipQps =
      typeof config.client_ip_qps === "number" && config.client_ip_qps > 0
        ? config.client_ip_qps
        : tenantQps;
    const ipBurst =
      typeof config.client_ip_burst === "number" && config.client_ip_burst > 0
        ? config.client_ip_burst
        : burst;
    setEnabled(config.enabled);
    setMaxConcurrent(tenantQps);
    setBurstSize(burst);
    setClientIpQps(ipQps);
    setClientIpBurst(ipBurst);
    setMaxBandwidthMbps(config.max_bandwidth_mbps);
    setExpectedUpdatedAt(config.updated_at);
    setClientIpManuallyEdited(ipQps !== tenantQps || ipBurst !== burst);
    setApiLimits(
      Object.entries(config.api_limits ?? {}).map(([path, rule]) => ({
        path,
        qps: rule.qps,
        burst: rule.burst,
      })),
    );
  }, []);

  const loadConfig = useCallback(async () => {
    if (!tenantCode) return;
    setLoading(true);
    try {
      const config = await fetchTenantRateLimitConfig(tenantCode);
      applyConfig(config);
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to load rate limit config:", error);
      }
    } finally {
      setLoading(false);
    }
  }, [tenantCode, applyConfig]);

  const loadCommands = useCallback(async () => {
    if (!tenantCode || !canWrite) {
      setCommands([]);
      return;
    }
    setCommandsLoading(true);
    try {
      const res = await fetchTenantRateLimitCommands(tenantCode, 50);
      setCommands(res.commands ?? []);
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to load rate limit commands:", error);
      }
    } finally {
      setCommandsLoading(false);
    }
  }, [tenantCode, canWrite]);

  useEffect(() => {
    void loadConfig();
    void loadCommands();
  }, [loadConfig, loadCommands]);

  const activeCommandCount = useMemo(
    () => commands.filter((c) => c.status === "active").length,
    [commands],
  );

  const handleTenantQpsChange = (nextVal: number) => {
    setMaxConcurrent(nextVal);
    if (!clientIpManuallyEdited || clientIpQps > nextVal) {
      setClientIpQps(nextVal);
    }
    setApiLimits((prev) =>
      prev.map((item) =>
        item.qps > nextVal ? { ...item, qps: nextVal } : item,
      ),
    );
  };

  const handleTenantBurstChange = (nextVal: number) => {
    setBurstSize(nextVal);
    if (!clientIpManuallyEdited || clientIpBurst > nextVal) {
      setClientIpBurst(nextVal);
    }
    setApiLimits((prev) =>
      prev.map((item) =>
        item.burst > nextVal ? { ...item, burst: nextVal } : item,
      ),
    );
  };

  const handleApplyTemplate = (tpl: PlanRateTemplate) => {
    setEnabled(tpl.enabled);
    setMaxConcurrent(tpl.max_concurrent);
    setBurstSize(tpl.burst_size);
    setMaxBandwidthMbps(tpl.max_bandwidth_mbps);
    setClientIpQps(tpl.client_ip_qps ?? tpl.max_concurrent);
    setClientIpBurst(tpl.client_ip_burst ?? tpl.burst_size);
    setClientIpManuallyEdited(false);
    if (tpl.clearApiLimits) {
      setApiLimits([]);
    }
    showSuccessToast(
      t("rateLimit.templates.applied", { name: t(tpl.nameKey) }),
    );
  };

  const validateApiOverrides = (): boolean => {
    for (const item of apiLimits) {
      const path = item.path.trim();
      if (!path) continue;
      if (!Number.isFinite(item.qps) || item.qps < 1) {
        showWarningToast(t("rateLimit.overrideInvalidLimit", { path }));
        return false;
      }
      if (!Number.isFinite(item.burst) || item.burst < 1) {
        showWarningToast(t("rateLimit.overrideInvalidBurst", { path }));
        return false;
      }
      if (item.qps > maxConcurrent) {
        showWarningToast(
          t("rateLimit.overrideExceedsPlan", {
            path,
            limit: item.qps,
            planQps: maxConcurrent,
          }),
        );
        return false;
      }
      if (item.burst > burstSize) {
        showWarningToast(
          t("rateLimit.overrideExceedsBurst", {
            path,
            burst: item.burst,
            planBurst: burstSize,
          }),
        );
        return false;
      }
    }
    return true;
  };

  const handleSave = async () => {
    if (saving || !canWrite) return;
    if (!validateApiOverrides()) {
      setApiOverridesExpanded(true);
      return;
    }

    const api_limits: Record<string, { qps: number; burst: number }> = {};
    for (const item of apiLimits) {
      const path = item.path.trim();
      if (path) {
        api_limits[path] = { qps: item.qps, burst: item.burst };
      }
    }

    const payload: UpdateTenantRateLimitConfigRequest = {
      enabled,
      max_concurrent: maxConcurrent,
      burst_size: burstSize,
      client_ip_qps: clientIpQps,
      client_ip_burst: clientIpBurst,
      max_bandwidth_mbps: maxBandwidthMbps,
      api_limits,
      expected_updated_at: expectedUpdatedAt,
    };

    setSaving(true);
    try {
      const saved = await updateTenantRateLimitConfig(tenantCode, payload);
      applyConfig(saved);
      showSuccessToast(t("common.feedback.saveSuccess"));
    } catch (error) {
      if (isMfaStepUpCancelled(error)) return;
      if (error instanceof HttpError && error.code === 409) {
        try {
          const latest = await fetchTenantRateLimitConfig(tenantCode);
          applyConfig(latest);
          setConflictText(summarizeRateLimitConflict(payload, latest));
        } catch {
          // ignore secondary fetch error
        }
      }
      logger.error("Failed to save rate limit config:", error);
    } finally {
      setSaving(false);
    }
  };

  const handleConfirmOps = async () => {
    if (!pendingOps || opsBusy) return;
    setOpsBusy(true);
    try {
      if (pendingOps.type === "deny") {
        await createTenantRateLimitCommand(tenantCode, {
          command_type: "deny_until",
          deny_for_secs: Math.round(denyMinutes * 60),
          reason: commandReason.trim() || undefined,
        });
      } else if (pendingOps.type === "quota") {
        await createTenantRateLimitCommand(tenantCode, {
          command_type: "quota_correction",
          quota_multiplier: quotaMultiplier,
          expires_in_secs: Math.round(quotaExpiresMinutes * 60),
          reason: commandReason.trim() || undefined,
        });
      } else if (pendingOps.type === "refresh") {
        await createTenantRateLimitCommand(tenantCode, {
          command_type: "refresh_policy",
          reason: commandReason.trim() || undefined,
        });
      } else if (pendingOps.type === "cancel") {
        await cancelTenantRateLimitCommand(tenantCode, pendingOps.row.id);
      }
      setPendingOps(null);
      await loadCommands();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to execute rate limit command:", error);
      }
    } finally {
      setOpsBusy(false);
    }
  };

  const pendingOpsDescription = useMemo(() => {
    if (!pendingOps) return "";
    if (pendingOps.type === "deny") return t("rateLimit.confirmDeny");
    if (pendingOps.type === "quota") return t("rateLimit.confirmQuota");
    if (pendingOps.type === "refresh") return t("rateLimit.confirmRefresh");
    return t("rateLimit.confirmCancel");
  }, [pendingOps, t]);

  return (
    <EntityDetailCard
      title={
        <span className="inline-flex items-center gap-1.5">
          <span>{t("rateLimit.title")}</span>
          <Tooltip content={t("rateLimit.clusterNote")}>
            <span className="inline-flex text-muted-foreground hover:text-foreground cursor-help">
              <Info className="h-3.5 w-3.5" />
            </span>
          </Tooltip>
        </span>
      }
      icon={<Gauge className="h-4 w-4" />}
      testId="tenant-rate-limit-card"
      actions={
        canWrite ? (
          <Button
            type="button"
            variant="primary"
            size="sm"
            loading={saving}
            disabled={loading}
            onClick={() => void handleSave()}
          >
            <Save className="h-3.5 w-3.5" />
            {t("common.actions.save")}
          </Button>
        ) : undefined
      }
    >
      <div className="space-y-4">
        {/* Enable Plan */}
        <div className="flex items-center justify-between gap-4 rounded-md border border-border-subtle bg-muted/25 px-3 py-2.5">
          <span className="text-xs font-medium text-foreground">
            {t("rateLimit.enablePlan")}
          </span>
          <Switch
            checked={enabled}
            disabled={!canWrite}
            onCheckedChange={setEnabled}
            aria-label={t("rateLimit.enablePlan")}
          />
        </div>

        {/* Plan Templates — Single non-wrapping row */}
        {canWrite && (
          <div className="flex items-center justify-between gap-3 border-t border-border-subtle pt-3">
            <span className="shrink-0 whitespace-nowrap text-xs font-medium text-foreground">
              {t("rateLimit.templates.label")}
            </span>
            <div
              className="flex flex-nowrap items-center gap-1.5 overflow-x-auto"
              data-testid="rate-limit-templates-row"
            >
              {PLAN_RATE_TEMPLATES.map((tpl) => (
                <ActionChip
                  key={tpl.id}
                  size="sm"
                  className="whitespace-nowrap shrink-0"
                  onClick={() => handleApplyTemplate(tpl)}
                >
                  {t(tpl.nameKey)}
                </ActionChip>
              ))}
            </div>
          </div>
        )}

        {/* Threshold Settings */}
        <div
          className={cn(
            "space-y-3 border-t border-border-subtle pt-3 transition-opacity",
            (!enabled || !canWrite) && "opacity-60",
          )}
        >
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <div className="flex items-center justify-between gap-2">
              <span className="text-xs text-foreground">
                {t("rateLimit.planQps")}
              </span>
              <NumberInput
                value={maxConcurrent}
                min={1}
                step={10}
                disabled={!enabled || !canWrite}
                onChange={handleTenantQpsChange}
                className="w-32"
                aria-label={t("rateLimit.planQps")}
              />
            </div>

            <div className="flex items-center justify-between gap-2">
              <span className="text-xs text-foreground">
                {t("rateLimit.planBurst")}
              </span>
              <NumberInput
                value={burstSize}
                min={1}
                step={10}
                disabled={!enabled || !canWrite}
                onChange={handleTenantBurstChange}
                className="w-32"
                aria-label={t("rateLimit.planBurst")}
              />
            </div>

            <div className="flex items-center justify-between gap-2">
              <span className="text-xs text-foreground">
                {t("rateLimit.clientIpQps")}
              </span>
              <NumberInput
                value={clientIpQps}
                min={1}
                max={maxConcurrent > 0 ? maxConcurrent : undefined}
                step={10}
                disabled={!enabled || !canWrite}
                onChange={(next) => {
                  setClientIpManuallyEdited(true);
                  setClientIpQps(Math.min(next, maxConcurrent));
                }}
                className="w-32"
                aria-label={t("rateLimit.clientIpQps")}
              />
            </div>

            <div className="flex items-center justify-between gap-2">
              <span className="text-xs text-foreground">
                {t("rateLimit.clientIpBurst")}
              </span>
              <NumberInput
                value={clientIpBurst}
                min={1}
                max={burstSize > 0 ? burstSize : undefined}
                step={10}
                disabled={!enabled || !canWrite}
                onChange={(next) => {
                  setClientIpManuallyEdited(true);
                  setClientIpBurst(Math.min(next, burstSize));
                }}
                className="w-32"
                aria-label={t("rateLimit.clientIpBurst")}
              />
            </div>

            <div className="flex items-center justify-between gap-2 sm:col-span-2">
              <span className="text-xs text-foreground">
                {t("rateLimit.bandwidthMbps")}
              </span>
              <NumberInput
                value={maxBandwidthMbps}
                min={0}
                step={10}
                disabled={!enabled || !canWrite}
                onChange={setMaxBandwidthMbps}
                className="w-32"
                aria-label={t("rateLimit.bandwidthMbps")}
              />
            </div>
          </div>

          {/* Collapsible API Overrides */}
          <div className="overflow-hidden rounded-md border border-border">
            <button
              type="button"
              disabled={!enabled || !canWrite}
              onClick={() => setApiOverridesExpanded((v) => !v)}
              className="flex w-full items-center gap-1.5 bg-muted/30 px-3 py-2 text-left text-xs font-medium text-foreground transition-colors hover:bg-hover-bg disabled:cursor-not-allowed disabled:opacity-55 cursor-pointer"
            >
              {apiOverridesExpanded ? (
                <ChevronDown className="h-3.5 w-3.5 text-muted-foreground" />
              ) : (
                <ChevronRight className="h-3.5 w-3.5 text-muted-foreground" />
              )}
              <span>{t("rateLimit.apiOverridesSection")}</span>
              {apiLimits.length > 0 && (
                <Badge variant="neutral">{apiLimits.length}</Badge>
              )}
            </button>

            {apiOverridesExpanded && (
              <div className="space-y-2 border-t border-border p-3">
                {canWrite && (
                  <div className="flex justify-end">
                    <Button
                      type="button"
                      variant="link"
                      size="xs"
                      disabled={!enabled}
                      onClick={() =>
                        setApiLimits((prev) => [
                          ...prev,
                          {
                            path: "",
                            qps: Math.min(1000, maxConcurrent),
                            burst: Math.min(1000, burstSize),
                          },
                        ])
                      }
                    >
                      <Plus className="h-3 w-3" />
                      {t("rateLimit.addOverride")}
                    </Button>
                  </div>
                )}

                {apiLimits.length === 0 ? (
                  <div className="py-3 text-center text-xs text-muted-foreground">
                    {t("rateLimit.apiOverridesEmpty")}
                  </div>
                ) : (
                  <div className="overflow-x-auto rounded-md border border-border">
                    <table className="w-full border-collapse text-xs">
                      <thead>
                        <tr className="border-b border-border bg-muted/45 text-muted-foreground">
                          <th className="h-8 px-2 text-left font-medium">
                            {t("rateLimit.columns.path")}
                          </th>
                          <th className="h-8 w-28 px-2 text-left font-medium">
                            {t("rateLimit.columns.qps")}
                          </th>
                          <th className="h-8 w-28 px-2 text-left font-medium">
                            {t("rateLimit.columns.burst")}
                          </th>
                          {canWrite && (
                            <th className="h-8 w-10 px-2 text-center font-medium">
                              {t("rateLimit.columns.operations")}
                            </th>
                          )}
                        </tr>
                      </thead>
                      <tbody className="divide-y divide-border-subtle">
                        {apiLimits.map((row, idx) => (
                          <tr key={idx}>
                            <td className="px-2 py-1.5">
                              <Input
                                value={row.path}
                                disabled={!enabled || !canWrite}
                                onChange={(e) =>
                                  setApiLimits((prev) =>
                                    prev.map((item, i) =>
                                      i === idx
                                        ? { ...item, path: e.target.value }
                                        : item,
                                    ),
                                  )
                                }
                                placeholder={t("rateLimit.pathPlaceholder")}
                                className="font-mono-tech"
                              />
                            </td>
                            <td className="px-2 py-1.5">
                              <NumberInput
                                value={row.qps}
                                min={1}
                                max={
                                  maxConcurrent > 0 ? maxConcurrent : undefined
                                }
                                disabled={!enabled || !canWrite}
                                onChange={(next) =>
                                  setApiLimits((prev) =>
                                    prev.map((item, i) =>
                                      i === idx ? { ...item, qps: next } : item,
                                    ),
                                  )
                                }
                                aria-label={t("rateLimit.columns.qps")}
                              />
                            </td>
                            <td className="px-2 py-1.5">
                              <NumberInput
                                value={row.burst}
                                min={1}
                                max={burstSize > 0 ? burstSize : undefined}
                                disabled={!enabled || !canWrite}
                                onChange={(next) =>
                                  setApiLimits((prev) =>
                                    prev.map((item, i) =>
                                      i === idx
                                        ? { ...item, burst: next }
                                        : item,
                                    ),
                                  )
                                }
                                aria-label={t("rateLimit.columns.burst")}
                              />
                            </td>
                            {canWrite && (
                              <td className="px-2 py-1.5 text-center">
                                <IconButton
                                  size="sm"
                                  disabled={!enabled}
                                  aria-label={t("common.actions.delete")}
                                  onClick={() =>
                                    setApiLimits((prev) =>
                                      prev.filter((_, i) => i !== idx),
                                    )
                                  }
                                  className="text-danger-fg hover:text-danger-fg"
                                >
                                  <Trash2 className="h-3.5 w-3.5" />
                                </IconButton>
                              </td>
                            )}
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                )}
              </div>
            )}
          </div>
        </div>

        {/* Emergency Ops Panel */}
        {canWrite && (
          <div className="overflow-hidden rounded-md border border-border">
            <button
              type="button"
              onClick={() => setOpsExpanded((v) => !v)}
              className="flex w-full items-center gap-1.5 bg-muted/30 px-3 py-2 text-left text-xs font-medium text-foreground transition-colors hover:bg-hover-bg cursor-pointer"
            >
              {opsExpanded ? (
                <ChevronDown className="h-3.5 w-3.5 text-muted-foreground" />
              ) : (
                <ChevronRight className="h-3.5 w-3.5 text-muted-foreground" />
              )}
              <span>{t("rateLimit.opsTitle")}</span>
              {activeCommandCount > 0 && (
                <Badge variant="warning">{activeCommandCount}</Badge>
              )}
            </button>

            {opsExpanded && (
              <div className="space-y-3 border-t border-border p-3">
                <div className="flex justify-end">
                  <Button
                    type="button"
                    variant="link"
                    size="xs"
                    loading={commandsLoading}
                    onClick={() => void loadCommands()}
                  >
                    <RefreshCw className="h-3 w-3" />
                    {t("common.actions.refresh")}
                  </Button>
                </div>

                <div className="space-y-2 rounded-md border border-border-subtle bg-muted/20 p-2.5">
                  <div className="flex flex-wrap items-center gap-2">
                    <NumberInput
                      value={denyMinutes}
                      min={1}
                      max={1440}
                      step={5}
                      onChange={setDenyMinutes}
                      className="w-28"
                      aria-label={t("rateLimit.denyForMinutes")}
                    />
                    <span className="shrink-0 whitespace-nowrap text-xs text-muted-foreground">
                      {t("rateLimit.denyForMinutes")}
                    </span>
                    <Button
                      type="button"
                      variant="danger"
                      size="sm"
                      loading={opsBusy}
                      onClick={() => setPendingOps({ type: "deny" })}
                    >
                      {t("rateLimit.issueDeny")}
                    </Button>
                  </div>

                  <div className="flex flex-wrap items-center gap-2">
                    <NumberInput
                      value={quotaMultiplier}
                      min={0.1}
                      max={10}
                      step={0.1}
                      onChange={setQuotaMultiplier}
                      className="w-28"
                      aria-label={t("rateLimit.quotaMultiplier")}
                    />
                    <span className="shrink-0 whitespace-nowrap text-xs text-muted-foreground">
                      {t("rateLimit.quotaMultiplier")}
                    </span>
                    <NumberInput
                      value={quotaExpiresMinutes}
                      min={1}
                      max={10080}
                      step={10}
                      onChange={setQuotaExpiresMinutes}
                      className="w-28"
                      aria-label={t("rateLimit.expiresInMinutes")}
                    />
                    <span className="shrink-0 whitespace-nowrap text-xs text-muted-foreground">
                      {t("rateLimit.expiresInMinutes")}
                    </span>
                    <Button
                      type="button"
                      variant="warning"
                      size="sm"
                      loading={opsBusy}
                      onClick={() => setPendingOps({ type: "quota" })}
                    >
                      {t("rateLimit.issueQuota")}
                    </Button>
                  </div>

                  <div className="flex flex-wrap items-center gap-2">
                    <Input
                      value={commandReason}
                      onChange={(e) => setCommandReason(e.target.value)}
                      placeholder={t("rateLimit.reason")}
                      className="min-w-[160px] flex-1"
                    />
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      loading={opsBusy}
                      onClick={() => setPendingOps({ type: "refresh" })}
                    >
                      {t("rateLimit.issueRefresh")}
                    </Button>
                  </div>
                </div>

                {commands.length === 0 ? (
                  <div className="py-3 text-center text-xs text-muted-foreground">
                    {t("rateLimit.noCommands")}
                  </div>
                ) : (
                  <div className="overflow-x-auto rounded-md border border-border">
                    <table className="w-full border-collapse text-xs">
                      <thead>
                        <tr className="border-b border-border bg-muted/45 text-muted-foreground">
                          <th className="h-8 px-2.5 text-left font-medium">
                            {t("rateLimit.columns.type")}
                          </th>
                          <th className="h-8 px-2.5 text-left font-medium">
                            {t("rateLimit.columns.status")}
                          </th>
                          <th className="h-8 px-2.5 text-left font-medium">
                            {t("rateLimit.columns.expires")}
                          </th>
                          <th className="h-8 px-2.5 text-left font-medium">
                            {t("rateLimit.columns.created")}
                          </th>
                          <th className="h-8 px-2.5 text-right font-medium">
                            {t("rateLimit.columns.operations")}
                          </th>
                        </tr>
                      </thead>
                      <tbody className="divide-y divide-border-subtle">
                        {commands.map((row) => (
                          <tr key={row.id}>
                            <td className="px-2.5 py-2">
                              {commandTypeLabel(row.command_type)}
                            </td>
                            <td className="px-2.5 py-2">
                              <Badge
                                variant={
                                  row.status === "active"
                                    ? "success"
                                    : "neutral"
                                }
                              >
                                {row.status === "active"
                                  ? t("rateLimit.statusActive")
                                  : t("rateLimit.statusExpired")}
                              </Badge>
                            </td>
                            <td className="px-2.5 py-2 font-mono-tech">
                              {formatTime(row.expires_at)}
                            </td>
                            <td className="px-2.5 py-2 font-mono-tech">
                              {formatTime(row.created_at)}
                            </td>
                            <td className="px-2.5 py-2 text-right">
                              {row.status === "active" && (
                                <Button
                                  type="button"
                                  variant="link"
                                  size="xs"
                                  className="text-danger-fg hover:text-danger-fg"
                                  loading={opsBusy}
                                  onClick={() =>
                                    setPendingOps({ type: "cancel", row })
                                  }
                                >
                                  {t("rateLimit.cancelCommand")}
                                </Button>
                              )}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                )}
              </div>
            )}
          </div>
        )}

        {/* Reason Guide */}
        <div className="overflow-hidden rounded-md border border-border">
          <button
            type="button"
            onClick={() => setReasonGuideExpanded((v) => !v)}
            className="flex w-full items-center gap-1.5 bg-muted/30 px-3 py-2 text-left text-xs font-medium text-foreground transition-colors hover:bg-hover-bg cursor-pointer"
          >
            {reasonGuideExpanded ? (
              <ChevronDown className="h-3.5 w-3.5 text-muted-foreground" />
            ) : (
              <ChevronRight className="h-3.5 w-3.5 text-muted-foreground" />
            )}
            <span>{t("rateLimit.reasons.title")}</span>
          </button>

          {reasonGuideExpanded && (
            <div className="divide-y divide-border-subtle border-t border-border px-3 py-1">
              {RATE_LIMIT_REASON_GUIDE.map((item) => {
                const loc = reasonNavLocation(item.target, tenantCode);
                const to = loc.query?.tab
                  ? `${loc.path}?tab=${encodeURIComponent(loc.query.tab)}`
                  : loc.path;
                return (
                  <div
                    key={item.reason}
                    className="flex flex-wrap items-center gap-2 py-2 text-xs"
                  >
                    <code className="shrink-0 rounded bg-muted px-1.5 py-0.5 font-mono-tech text-[11px] text-primary">
                      {item.reason}
                    </code>
                    <span className="min-w-0 flex-1 text-muted-foreground">
                      {t(item.talkKey)}
                    </span>
                    <Link
                      to={to}
                      className="shrink-0 font-medium text-primary hover:underline"
                    >
                      {t("rateLimit.reasons.goConfig")}
                    </Link>
                  </div>
                );
              })}
            </div>
          )}
        </div>

        {/* Upstream Footer Hint */}
        <div className="text-[11px] text-muted-foreground">
          <span>{t("rateLimit.upstreamFooter")}</span>
          <Link
            to="/operations/upstream-supply"
            className="ml-1 font-medium text-primary hover:underline"
          >
            {t("rateLimit.upstreamFooterLink")}
          </Link>
        </div>
      </div>

      <ConfirmDialog
        open={Boolean(pendingOps)}
        onClose={() => setPendingOps(null)}
        onConfirm={() => void handleConfirmOps()}
        loading={opsBusy}
        danger={pendingOps?.type === "deny" || pendingOps?.type === "cancel"}
        title={t("common.tips")}
        confirmLabel={t("common.confirm")}
        description={pendingOpsDescription}
      />

      <Modal
        open={Boolean(conflictText)}
        onClose={() => setConflictText(null)}
        title={
          <span className="inline-flex items-center gap-1.5">
            <AlertTriangle className="h-4 w-4 text-warning-fg" />
            <span>{t("common.conflict.title")}</span>
          </span>
        }
        size="sm"
        footer={
          <Button
            type="button"
            variant="primary"
            onClick={() => setConflictText(null)}
          >
            {t("common.confirm")}
          </Button>
        }
      >
        <pre className="whitespace-pre-wrap rounded-md border border-border bg-muted/35 p-3 font-mono-tech text-xs text-foreground">
          {conflictText}
        </pre>
      </Modal>
    </EntityDetailCard>
  );
}
