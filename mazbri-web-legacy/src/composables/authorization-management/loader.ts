import { ElMessage } from "element-plus";
import { ref } from "vue";
import { fetchLicenseStatus } from "@/api/authorizations";
import { runWithLoading } from "@/composables/useAsyncState";
import type { LicenseStatus } from "@/types/api/authorizations";
import { $t } from "@/locales";
import { getUserFacingErrorMessage } from "@/utils/http/error";
import { logger } from "@/utils/sys";

export function useAuthorizationManagementLoader() {
  const currentSummary = ref<LicenseStatus | null>(null);
  const refreshing = ref(false);

  async function loadStatus(useLoading = true) {
    const task = async () => {
      try {
        currentSummary.value = await fetchLicenseStatus();
      } catch (error) {
        logger.error("Failed to load license status:", error);
        ElMessage.error(
          getUserFacingErrorMessage(error, $t("httpMsg.requestFailed")),
        );
      }
    };
    if (useLoading) {
      await runWithLoading(refreshing, task);
      return;
    }
    await task();
  }

  return {
    currentSummary,
    loadStatus,
    refreshing,
  };
}
