import { ElMessageBox } from "element-plus";
import { $t } from "@/locales";
export const MfaDialogService = {
  async requestVerification(): Promise<string> {
    const { value } = await ElMessageBox.prompt(
      $t("common.mfaDialog.placeholder"),
      $t("common.mfaDialog.title"),
      {
        confirmButtonText: $t("common.confirm"),
        cancelButtonText: $t("common.cancel"),
        inputPattern: /^\d{6,8}$/,
        inputErrorMessage: $t("common.mfaDialog.inputErrorMessage"),
        inputType: "password",
        distinguishCancelAndClose: true,
        closeOnClickModal: false,
        closeOnPressEscape: false,
      },
    );
    return String(value).trim();
  },
};
