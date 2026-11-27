import { useCallback, useEffect, useMemo, useState } from "react";
import { Globe, Loader2, ShieldCheck } from "lucide-react";
import { fetchSystemSettings, updateSystemSettings } from "@/api/settings";
import {
  Input,
  NumberInput,
  SettingRow,
  SettingsSectionCard,
  SettingsSectionLayout,
  Switch,
} from "@/components/ui";
import { useT } from "@/locales";
import { useWatermarkStore } from "@/stores/watermark";
import type { UpdateSystemSettingsRequest } from "@/types/api/settings";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { logger } from "@/utils/sys/logger";
import {
  showSuccessToast,
  showWarningToast,
} from "@/utils/ui/message-guard";

const BYTES_PER_MB = 1024 * 1024;

interface BasicSystemFormState {
  site_name: string;
  site_url: string;
  tenant_gateway_url: string;
  session_timeout_minutes: number;
  max_upload_size_mb: number;
  enable_audit_log: boolean;
  watermark_visible: boolean;
}

const DEFAULT_FORM: BasicSystemFormState = {
  site_name: "",
  site_url: "",
  tenant_gateway_url: "",
  session_timeout_minutes: 120,
  max_upload_size_mb: 10,
  enable_audit_log: true,
  watermark_visible: false,
};

export interface BasicSettingsTabProps {
  onDirtyChange: (dirty: boolean) => void;
}

export function BasicSettingsTab({ onDirtyChange }: BasicSettingsTabProps) {
  const { t } = useT();
  const [form, setForm] = useState<BasicSystemFormState>(DEFAULT_FORM);
  const [baseline, setBaseline] = useState<BasicSystemFormState | null>(null);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);

  const loadSettings = useCallback(async () => {
    setLoading(true);
    try {
      const res = await fetchSystemSettings(true);
      const next: BasicSystemFormState = {
        site_name: res.site_name || "",
        site_url: res.site_url || "",
        tenant_gateway_url: res.tenant_gateway_url || "",
        session_timeout_minutes: res.session_timeout
          ? Math.max(1, Math.round(res.session_timeout / 60000))
          : 120,
        max_upload_size_mb: res.max_upload_size
          ? Math.max(1, Math.round(res.max_upload_size / BYTES_PER_MB))
          : 10,
        enable_audit_log: res.enable_audit_log ?? true,
        watermark_visible: Boolean(res.watermark_visible),
      };
      setForm(next);
      setBaseline(next);
    } catch (error) {
      logger.error("Failed to load system settings:", error);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  const isDirty = useMemo(
    () => baseline !== null && JSON.stringify(form) !== JSON.stringify(baseline),
    [form, baseline],
  );

  useEffect(() => {
    onDirtyChange(isDirty);
    return () => onDirtyChange(false);
  }, [isDirty, onDirtyChange]);

  const handleReset = () => {
    if (baseline) {
      setForm(baseline);
    }
  };

  const handleSave = async () => {
    if (saving) return;
    const siteUrl = form.site_url.trim();
    if (siteUrl && !/^https?:\/\//i.test(siteUrl)) {
      showWarningToast(t("systemSettings.validation.siteUrlInvalid"));
      return;
    }
    const gatewayUrl = form.tenant_gateway_url.trim();
    if (gatewayUrl && !/^https?:\/\//i.test(gatewayUrl)) {
      showWarningToast(t("systemSettings.validation.tenantGatewayUrlInvalid"));
      return;
    }

    setSaving(true);
    try {
      const payload: UpdateSystemSettingsRequest = {
        site_name: form.site_name.trim() || undefined,
        site_url: siteUrl,
        tenant_gateway_url: gatewayUrl || undefined,
        session_timeout: form.session_timeout_minutes * 60000,
        max_upload_size: form.max_upload_size_mb * BYTES_PER_MB,
        enable_audit_log: Boolean(form.enable_audit_log),
        watermark_visible: Boolean(form.watermark_visible),
      };
      await updateSystemSettings(payload);
      await useWatermarkStore.getState().loadFromServer(true);
      setBaseline(form);
      showSuccessToast(t("systemSettings.messages.savedSuccess"));
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to save system settings:", error);
      }
    } finally {
      setSaving(false);
    }
  };

  if (loading && !baseline) {
    return (
      <div className="flex items-center justify-center py-16 text-xs text-muted-foreground gap-2">
        <Loader2 className="h-4 w-4 animate-spin text-primary" />
        <span>{t("layout.loading")}</span>
      </div>
    );
  }

  return (
    <SettingsSectionLayout
      testId="basic-settings-tab"
      columns={2}
      dirty={isDirty}
      saving={saving}
      onSave={() => void handleSave()}
      onReset={handleReset}
      dirtyLabel={t("systemSettings.unsavedChanges")}
      saveLabel={t("common.actions.save")}
      resetLabel={t("common.reset")}
    >
      <SettingsSectionCard
        title={t("systemSettings.sections.basic")}
        icon={<Globe className="h-4 w-4" />}
      >
        <SettingRow
          label={t("systemSettings.form.siteName")}
          htmlFor="setting-site-name"
        >
          <Input
            id="setting-site-name"
            value={form.site_name}
            onChange={(e) =>
              setForm((prev) => ({ ...prev, site_name: e.target.value }))
            }
            placeholder="Mazbri Console"
          />
        </SettingRow>

        <SettingRow
          label={t("systemSettings.form.siteUrl")}
          htmlFor="setting-site-url"
        >
          <Input
            id="setting-site-url"
            value={form.site_url}
            onChange={(e) =>
              setForm((prev) => ({ ...prev, site_url: e.target.value }))
            }
            placeholder="https://console.example.com"
          />
        </SettingRow>

        <SettingRow
          label={t("systemSettings.form.tenantGatewayUrl")}
          htmlFor="setting-gateway-url"
          hint={t("systemSettings.form.tenantGatewayUrlHint")}
        >
          <Input
            id="setting-gateway-url"
            value={form.tenant_gateway_url}
            onChange={(e) =>
              setForm((prev) => ({
                ...prev,
                tenant_gateway_url: e.target.value,
              }))
            }
            placeholder="https://gateway.example.com"
          />
        </SettingRow>
      </SettingsSectionCard>

      <SettingsSectionCard
        title={t("systemSettings.sections.runtimeSecurity")}
        icon={<ShieldCheck className="h-4 w-4" />}
      >
        <SettingRow
          label={t("systemSettings.form.sessionTimeout")}
          layout="inline"
        >
          <NumberInput
            value={form.session_timeout_minutes}
            min={1}
            max={1440}
            onChange={(next) =>
              setForm((prev) => ({ ...prev, session_timeout_minutes: next }))
            }
            aria-label={t("systemSettings.form.sessionTimeout")}
            className="w-32"
          />
        </SettingRow>

        <SettingRow
          label={t("systemSettings.form.maxUploadSize")}
          layout="inline"
        >
          <NumberInput
            value={form.max_upload_size_mb}
            min={1}
            max={512}
            onChange={(next) =>
              setForm((prev) => ({ ...prev, max_upload_size_mb: next }))
            }
            aria-label={t("systemSettings.form.maxUploadSize")}
            className="w-32"
          />
        </SettingRow>

        <div className="rounded-lg border border-border divide-y divide-border bg-card">
          <div className="px-3.5 py-2.5">
            <SettingRow
              label={t("systemSettings.form.enableAuditLog")}
              layout="inline"
            >
              <Switch
                checked={form.enable_audit_log}
                onCheckedChange={(checked) =>
                  setForm((prev) => ({ ...prev, enable_audit_log: checked }))
                }
                aria-label={t("systemSettings.form.enableAuditLog")}
              />
            </SettingRow>
          </div>
          <div className="px-3.5 py-2.5">
            <SettingRow
              label={t("systemSettings.form.watermarkVisible")}
              layout="inline"
            >
              <Switch
                checked={form.watermark_visible}
                onCheckedChange={(checked) =>
                  setForm((prev) => ({ ...prev, watermark_visible: checked }))
                }
                aria-label={t("systemSettings.form.watermarkVisible")}
              />
            </SettingRow>
          </div>
        </div>
      </SettingsSectionCard>
    </SettingsSectionLayout>
  );
}
