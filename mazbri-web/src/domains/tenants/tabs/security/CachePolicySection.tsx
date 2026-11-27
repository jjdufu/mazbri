import { useCallback, useEffect, useState } from "react";
import { Database, Plus, Save, Trash2 } from "lucide-react";
import { fetchTenantCacheConfig, updateTenantCacheConfig } from "@/api/tenant";
import {
  ActionChip,
  Button,
  EntityDetailCard,
  IconButton,
  Input,
  NumberInput,
  Switch,
} from "@/components/ui";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import type { TenantCacheConfig } from "@/types/api/tenant";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { isBizRole } from "@/utils/ops/roles";
import { logger } from "@/utils/sys/logger";
import { showSuccessToast } from "@/utils/ui/message-guard";

export interface CachePolicySectionProps {
  tenantCode: string;
}

const BYTES_PER_MB = 1_048_576;
const MAX_CACHE_SIZE_BYTES = 2_147_483_647;
const CACHE_SIZE_PRESETS_MB = [128, 512, 1024] as const;

function bytesToMb(bytes: number): number {
  return Math.min(2047, Math.max(1, Math.round(bytes / BYTES_PER_MB)));
}

function mbToBytes(mb: number): number {
  return Math.min(
    MAX_CACHE_SIZE_BYTES,
    Math.max(1, Math.round(Number.isFinite(mb) ? mb : 100)) * BYTES_PER_MB,
  );
}

export function CachePolicySection({ tenantCode }: CachePolicySectionProps) {
  const { t } = useT();
  const role = useAuthStore((s) => s.info.role);
  const canWrite = isBizRole(role);

  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);

  const [enabled, setEnabled] = useState(true);
  const [tenantIsolated, setTenantIsolated] = useState(true);
  const [defaultTtl, setDefaultTtl] = useState(300);
  const [cacheSizeMb, setCacheSizeMb] = useState(100);
  const [expectedUpdatedAt, setExpectedUpdatedAt] = useState<
    string | undefined
  >(undefined);
  const [warmupKeys, setWarmupKeys] = useState<string[]>([]);
  const [overrides, setOverrides] = useState<
    Array<{ path: string; ttl: number }>
  >([]);

  const applyConfig = useCallback((res: TenantCacheConfig) => {
    setEnabled(res.enabled ?? true);
    setDefaultTtl(res.default_ttl ?? 300);
    setCacheSizeMb(bytesToMb(res.max_cache_size ?? 100 * BYTES_PER_MB));
    setTenantIsolated(res.tenant_isolated ?? true);
    setExpectedUpdatedAt(res.updated_at);
    setWarmupKeys([...(res.warmup_keys ?? [])]);
    setOverrides(
      Object.entries(res.api_ttl ?? {}).map(([path, ttl]) => ({
        path,
        ttl: Number(ttl) || 0,
      })),
    );
  }, []);

  const loadData = useCallback(async () => {
    if (!tenantCode) return;
    setLoading(true);
    try {
      const res = await fetchTenantCacheConfig(tenantCode);
      if (res) applyConfig(res);
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to load cache policy:", error);
      }
    } finally {
      setLoading(false);
    }
  }, [tenantCode, applyConfig]);

  useEffect(() => {
    void loadData();
  }, [loadData]);

  const handleSave = async () => {
    if (saving || !canWrite) return;
    setSaving(true);
    try {
      const api_ttl: Record<string, number> = {};
      for (const item of overrides) {
        const trimmed = item.path.trim();
        if (trimmed) {
          api_ttl[trimmed] = item.ttl;
        }
      }
      const cleanedWarmup = warmupKeys.map((k) => k.trim()).filter(Boolean);
      const saved = await updateTenantCacheConfig(tenantCode, {
        enabled,
        default_ttl: defaultTtl,
        max_cache_size: mbToBytes(cacheSizeMb),
        tenant_isolated: tenantIsolated,
        api_ttl,
        warmup_enabled: cleanedWarmup.length > 0,
        warmup_keys: cleanedWarmup,
        expected_updated_at: expectedUpdatedAt,
      });
      if (saved) applyConfig(saved);
      showSuccessToast(t("common.feedback.saveSuccess"));
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to save cache policy:", error);
      }
    } finally {
      setSaving(false);
    }
  };

  return (
    <EntityDetailCard
      title={t("tenant.cachePolicy.title")}
      icon={<Database className="h-4 w-4" />}
      testId="tenant-cache-policy-card"
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
        {/* Toggle tiles */}
        <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
          <div className="flex items-center justify-between gap-3 rounded-md border border-border-subtle bg-muted/25 px-3 py-2.5">
            <span className="text-xs font-medium text-foreground">
              {t("tenant.cachePolicy.fields.enabled")}
            </span>
            <Switch
              checked={enabled}
              disabled={!canWrite}
              onCheckedChange={setEnabled}
              aria-label={t("tenant.cachePolicy.fields.enabled")}
            />
          </div>
          <div className="flex items-center justify-between gap-3 rounded-md border border-border-subtle bg-muted/25 px-3 py-2.5">
            <span className="text-xs font-medium text-foreground">
              {t("tenant.cachePolicy.fields.isolated")}
            </span>
            <Switch
              checked={tenantIsolated}
              disabled={!enabled || !canWrite}
              onCheckedChange={setTenantIsolated}
              aria-label={t("tenant.cachePolicy.fields.isolated")}
            />
          </div>
        </div>

        {/* Default TTL */}
        <div className="flex flex-wrap items-center justify-between gap-2 border-t border-border-subtle pt-3">
          <span className="text-xs font-medium text-foreground">
            {t("tenant.cachePolicy.fields.ttl")}
          </span>
          <div className="flex items-center gap-2">
            <NumberInput
              value={defaultTtl}
              onChange={setDefaultTtl}
              min={0}
              disabled={!canWrite}
              className="w-36"
              aria-label={t("tenant.cachePolicy.fields.ttl")}
            />
            <span className="shrink-0 whitespace-nowrap text-xs text-muted-foreground">
              {t("tenant.cachePolicy.seconds")}
            </span>
          </div>
        </div>

        {/* Max Cache Size */}
        <div className="flex flex-wrap items-start justify-between gap-2 border-t border-border-subtle pt-3">
          <span className="pt-1 text-xs font-medium text-foreground">
            {t("tenant.cachePolicy.fields.maxSize")}
          </span>
          <div className="flex flex-col items-end gap-2">
            <div className="flex items-center gap-2">
              <NumberInput
                value={cacheSizeMb}
                onChange={setCacheSizeMb}
                min={1}
                max={2047}
                step={64}
                disabled={!canWrite}
                className="w-36"
                aria-label={t("tenant.cachePolicy.fields.maxSize")}
              />
              <span className="shrink-0 whitespace-nowrap text-xs text-muted-foreground">
                MB
              </span>
            </div>
            {canWrite && (
              <div className="flex flex-nowrap items-center gap-1.5">
                {CACHE_SIZE_PRESETS_MB.map((presetMb) => (
                  <ActionChip
                    key={presetMb}
                    size="xs"
                    active={cacheSizeMb === presetMb}
                    onClick={() => setCacheSizeMb(presetMb)}
                  >
                    {presetMb} MB
                  </ActionChip>
                ))}
              </div>
            )}
          </div>
        </div>

        {/* Warmup Keys */}
        <div className="space-y-2 border-t border-border-subtle pt-3">
          <div className="flex items-center justify-between gap-2">
            <span className="text-xs font-medium text-foreground">
              {t("tenant.cachePolicy.warmupList")}
            </span>
            {canWrite && (
              <Button
                type="button"
                variant="link"
                size="xs"
                onClick={() => setWarmupKeys((prev) => [...prev, ""])}
              >
                <Plus className="h-3 w-3" />
                {t("tenant.cachePolicy.newWarmupKeyPlaceholder")}
              </Button>
            )}
          </div>

          {warmupKeys.length === 0 ? (
            <div className="rounded-md border border-dashed border-border py-4 text-center text-xs text-muted-foreground">
              {t("common.feedback.noData")}
            </div>
          ) : (
            <div className="space-y-1.5">
              {warmupKeys.map((val, idx) => (
                <div key={idx} className="flex items-center gap-2">
                  <Input
                    value={val}
                    disabled={!canWrite}
                    onChange={(e) =>
                      setWarmupKeys((prev) =>
                        prev.map((k, i) => (i === idx ? e.target.value : k)),
                      )
                    }
                    placeholder={t("tenant.cachePolicy.fields.warmupKeys")}
                    className="font-mono-tech"
                  />
                  {canWrite && (
                    <IconButton
                      size="sm"
                      aria-label={t("common.actions.delete")}
                      onClick={() =>
                        setWarmupKeys((prev) =>
                          prev.filter((_, i) => i !== idx),
                        )
                      }
                      className="text-danger-fg hover:text-danger-fg"
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </IconButton>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>

        {/* API TTL Overrides */}
        <div className="space-y-2 border-t border-border-subtle pt-3">
          <div className="flex items-center justify-between gap-2">
            <span className="text-xs font-medium text-foreground">
              {t("tenant.cachePolicy.apiOverrides")}
            </span>
            {canWrite && (
              <Button
                type="button"
                variant="link"
                size="xs"
                onClick={() =>
                  setOverrides((prev) => [...prev, { path: "", ttl: 300 }])
                }
              >
                <Plus className="h-3 w-3" />
                {t("tenant.cachePolicy.addRule")}
              </Button>
            )}
          </div>

          {overrides.length === 0 ? (
            <div className="rounded-md border border-dashed border-border py-4 text-center text-xs text-muted-foreground">
              {t("common.feedback.noData")}
            </div>
          ) : (
            <div className="overflow-x-auto rounded-md border border-border">
              <table className="w-full border-collapse text-xs">
                <thead>
                  <tr className="border-b border-border bg-muted/45 text-muted-foreground">
                    <th className="h-8 px-2.5 text-left font-medium">
                      {t("tenant.cachePolicy.fields.apiPath")}
                    </th>
                    <th className="h-8 w-40 px-2.5 text-left font-medium">
                      {t("tenant.cachePolicy.fields.overrideTtl")}
                    </th>
                    {canWrite && (
                      <th className="h-8 w-12 px-2 text-center font-medium">
                        {t("common.fields.operations")}
                      </th>
                    )}
                  </tr>
                </thead>
                <tbody className="divide-y divide-border-subtle">
                  {overrides.map((item, idx) => (
                    <tr key={idx}>
                      <td className="px-2 py-1.5">
                        <Input
                          value={item.path}
                          disabled={!canWrite}
                          onChange={(e) =>
                            setOverrides((prev) =>
                              prev.map((row, i) =>
                                i === idx
                                  ? { ...row, path: e.target.value }
                                  : row,
                              ),
                            )
                          }
                          placeholder={t(
                            "tenant.cachePolicy.apiPathPlaceholder",
                          )}
                          className="font-mono-tech"
                        />
                      </td>
                      <td className="px-2 py-1.5">
                        <div className="flex items-center gap-1.5">
                          <NumberInput
                            value={item.ttl}
                            min={0}
                            disabled={!canWrite}
                            onChange={(next) =>
                              setOverrides((prev) =>
                                prev.map((row, i) =>
                                  i === idx ? { ...row, ttl: next } : row,
                                ),
                              )
                            }
                            aria-label={t(
                              "tenant.cachePolicy.fields.overrideTtl",
                            )}
                          />
                          <span className="shrink-0 whitespace-nowrap text-muted-foreground">
                            {t("tenant.cachePolicy.seconds")}
                          </span>
                        </div>
                      </td>
                      {canWrite && (
                        <td className="px-2 py-1.5 text-center">
                          <IconButton
                            size="sm"
                            aria-label={t("common.actions.delete")}
                            onClick={() =>
                              setOverrides((prev) =>
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
      </div>
    </EntityDetailCard>
  );
}
