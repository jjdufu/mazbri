import { reactive, ref } from "vue";
import { ElMessage } from "element-plus";
import { fetchSystemSettings, updateSystemSettings } from "@/api/settings";
import { $t } from "@/locales";
import { logger } from "@/utils/sys";
import type { UpdateSystemSettingsRequest } from "@/types/api/settings";
import { useSettingStore } from "@/store/modules/setting";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-dialog-service";
import { runWithLoading } from "@/composables/useAsyncState";

const BYTES_PER_MB = 1024 * 1024;

export interface BasicSystemFormState {
  site_name: string;
  site_url: string;
  tenant_gateway_url: string;
  session_timeout_minutes: number;
  max_upload_size_mb: number;
  enable_audit_log: boolean;
  watermark_visible: boolean;
}

export function useSystemSettings() {
  const settingStore = useSettingStore();
  const loading = ref(false);
  const systemSaving = ref(false);

  const systemForm = reactive<BasicSystemFormState>({
    site_name: "",
    site_url: "",
    tenant_gateway_url: "",
    session_timeout_minutes: 120,
    max_upload_size_mb: 10,
    enable_audit_log: true,
    watermark_visible: false,
  });

  const validateBeforeSave = (): string | null => {
    const siteUrl = systemForm.site_url.trim();
    if (siteUrl && !/^https?:\/\//i.test(siteUrl)) {
      return $t("systemSettings.validation.siteUrlInvalid");
    }
    const gatewayUrl = systemForm.tenant_gateway_url.trim();
    if (gatewayUrl && !/^https?:\/\//i.test(gatewayUrl)) {
      return $t("systemSettings.validation.tenantGatewayUrlInvalid");
    }
    return null;
  };

  const loadSystemData = async () => {
    await runWithLoading(loading, async () => {
      const res = await fetchSystemSettings(true);
      Object.assign(systemForm, {
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
      });
    }).catch((error) => {
      logger.error("Failed to load system settings:", error);
    });
  };

  const saveSystemSettings = async (options?: {
    silent?: boolean;
  }): Promise<boolean> => {
    const validationError = validateBeforeSave();
    if (validationError) {
      ElMessage.warning(validationError);
      return false;
    }

    let saved = false;
    await runWithLoading(systemSaving, async () => {
      const payload: UpdateSystemSettingsRequest = {
        site_name: systemForm.site_name.trim() || undefined,
        site_url: systemForm.site_url.trim(),
        tenant_gateway_url: systemForm.tenant_gateway_url.trim() || undefined,
        session_timeout: systemForm.session_timeout_minutes * 60000,
        max_upload_size: systemForm.max_upload_size_mb * BYTES_PER_MB,
        enable_audit_log: Boolean(systemForm.enable_audit_log),
        watermark_visible: Boolean(systemForm.watermark_visible),
      };
      await updateSystemSettings(payload);
      await settingStore.loadSystemSettings(true);
      saved = true;
      if (!options?.silent) {
        ElMessage.success($t("systemSettings.messages.savedSuccess"));
      }
    }).catch((error) => {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to save system settings:", error);
      }
    });
    return saved;
  };

  const handleSystemSave = async () => {
    await saveSystemSettings();
  };

  return {
    loading,
    systemSaving,
    systemForm,
    loadSystemData,
    saveSystemSettings,
    handleSystemSave,
  };
}
