import { ElMessage } from "element-plus";
import { exportLicenseRequest, importLicense } from "@/api/authorizations";
import { runWithLoading } from "@/composables/useAsyncState";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-dialog-service";
import { $t } from "@/locales";
import { logger } from "@/utils/sys";
import type { AuthorizationManagementContext } from "./state";

export type AuthorizationFormState = {
  packageFile: File | null;
  licenseText: string;
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
      ElMessage.success(
        $t("certificate.authorizationManagement.license.exportSuccess"),
      );
    }).catch((error) => {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to export license request:", error);
      }
    });
  }

  async function handleImportLicense() {
    let file = options.state.form.packageFile;
    const rawText = options.state.form.licenseText.trim();
    if (!file && rawText) {
      file = new File([rawText], "license.jws", { type: "application/jose" });
    }
    if (!file) {
      ElMessage.warning(
        $t("certificate.authorizationManagement.license.selectFile"),
      );
      return;
    }
    await runWithLoading(options.state.importing, async () => {
      await importLicense(file);
      options.state.form.packageFile = null;
      options.state.form.licenseText = "";
      await options.state.loadStatus(false);
      ElMessage.success(
        $t("certificate.authorizationManagement.license.importSuccess"),
      );
    }).catch((error) => {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to import license:", error);
      }
    });
  }

  return {
    handleExportRequest,
    handleImportLicense,
  };
}
