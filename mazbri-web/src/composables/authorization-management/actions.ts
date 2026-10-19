import { ElMessage } from "element-plus";
import { exportLicenseRequest, importLicense } from "@/api/authorizations";
import { runWithLoading } from "@/composables/useAsyncState";
import { $t } from "@/locales";
import type { AuthorizationManagementContext } from "./state";

export type AuthorizationFormState = {
  packageFile: File | null;
  passphrase: string;
};

export function useAuthorizationManagementActions(options: {
  state: AuthorizationManagementContext;
}) {
  async function handleExportRequest() {
    await runWithLoading(options.state.refreshing, async () => {
      const requestFile = await exportLicenseRequest();
      const blob = new Blob([JSON.stringify(requestFile, null, 2)], {
        type: "application/json",
      });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = "mazbri-license-request.json";
      link.click();
      URL.revokeObjectURL(url);
      ElMessage.success($t("certificate.authorizationManagement.license.exportSuccess"));
    });
  }

  async function handleImportLicense() {
    const file = options.state.form.packageFile;
    if (!file) {
      ElMessage.warning($t("certificate.authorizationManagement.license.selectFile"));
      return;
    }
    await runWithLoading(options.state.importing, async () => {
      await importLicense(file);
      options.state.form.packageFile = null;
      await options.state.loadStatus(false);
      ElMessage.success($t("certificate.authorizationManagement.license.importSuccess"));
    });
  }

  return {
    handleExportRequest,
    handleImportLicense,
  };
}
