import { ref, reactive } from "vue";
import type { FormInstance, FormRules } from "element-plus";
import { ElMessage } from "element-plus";
import { fetchSystemSettings, updateSystemSettings } from "@/api/audit";
import { $t } from "@/locales";
import { logger } from "@/utils/sys";
import type { UpdateSystemSettingsRequest } from "@/types/api/settings";
import { useSettingStore } from "@/store/modules/setting";
import { runWithLoading } from "@/composables/useAsyncState";
export function useSystemSettings() {
  const settingStore = useSettingStore();
  const loading = ref(false);
  const systemSaving = ref(false);
  const systemFormRef = ref<FormInstance>();
  const systemForm = reactive<UpdateSystemSettingsRequest>({
    site_name: "",
    site_url: "",
    tenant_gateway_url: "",
    admin_email: "",
    session_timeout_minutes: 30,
    max_login_attempts: 5,
  });
  const systemRules: FormRules = {
    site_name: [],
    site_url: [
      {
        type: "url",
        message: $t("systemSettings.validation.siteUrlInvalid"),
        trigger: "blur",
      },
    ],
    tenant_gateway_url: [
      {
        validator: (_rule, value, callback) => {
          const v = String(value ?? "").trim();
          if (!v) {
            callback();
            return;
          }
          if (!/^https?:\/\//i.test(v)) {
            callback(
              new Error($t("systemSettings.validation.tenantGatewayUrlInvalid")),
            );
            return;
          }
          callback();
        },
        trigger: "blur",
      },
    ],
    admin_email: [
      {
        type: "email",
        message: $t("systemSettings.validation.adminEmailInvalid"),
        trigger: "blur",
      },
    ],
  };
  const loadSystemData = async () => {
    await runWithLoading(loading, async () => {
      const res = await fetchSystemSettings();
      const mappedData = {
        site_name: res.site_name || "",
        site_url: res.site_url || "",
        tenant_gateway_url: res.tenant_gateway_url || "",
        admin_email: res.admin_email || "",
        session_timeout_minutes: res.session_timeout
          ? Math.floor(res.session_timeout / 60000)
          : 30,
        max_login_attempts: res.max_login_attempts || 5,
      };
      Object.assign(systemForm, mappedData);
    }).catch((error) => {
      logger.error("Failed to load system settings:", error);
    });
  };
  const saveSystemSettings = async (options?: { silent?: boolean }) => {
    await runWithLoading(systemSaving, async () => {
      const payload = {
        site_name: systemForm.site_name || undefined,
        site_url: systemForm.site_url || undefined,
        tenant_gateway_url: systemForm.tenant_gateway_url?.trim() || undefined,
        session_timeout: systemForm.session_timeout_minutes
          ? systemForm.session_timeout_minutes * 60000
          : undefined,
      };
      await updateSystemSettings(payload);
      await settingStore.loadSystemSettings();
      if (!options?.silent) {
        ElMessage.success($t("systemSettings.messages.savedSuccess"));
      }
    }).catch((error) => {
      logger.error("Failed to save system settings:", error);
    });
    return true;
  };
  const handleSystemSave = async () => {
    await saveSystemSettings();
  };
  return {
    loading,
    systemSaving,
    systemFormRef,
    systemForm,
    systemRules,
    loadSystemData,
    saveSystemSettings,
    handleSystemSave,
  };
}
