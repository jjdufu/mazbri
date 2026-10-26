import { ElMessageBox } from "element-plus";
import { $t } from "@/locales";
import { router } from "@/router";
import { ACCOUNT_SECURITY_PATH, resolveMfaSetupPath } from "./mfa-setup-path";

export { ACCOUNT_SECURITY_PATH };

// 并发的多个敏感请求（如同一页面同时保存两类设置）同时收到 MFA_REQUIRED 时，
// 只弹出一次验证码框，所有请求复用同一个验证码重试。
let pendingVerification: Promise<string> | null = null;
// 同一时刻只保留一个"前往绑定 MFA"引导框，避免并发请求叠加弹窗。
let setupPromptOpen = false;

async function promptVerificationCode(): Promise<string> {
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
}

export const MfaDialogService = {
  requestVerification(): Promise<string> {
    if (!pendingVerification) {
      pendingVerification = promptVerificationCode().finally(() => {
        pendingVerification = null;
      });
    }
    return pendingVerification;
  },
  /**
   * 敏感操作返回 `MFA_NOT_ENABLED` 时引导用户前往账号安全页绑定 MFA。
   * 选择"前往绑定"后跳转到 `setupPath`，取消则留在当前页。
   */
  async promptSetup(setupPath: unknown): Promise<void> {
    if (setupPromptOpen) return;
    setupPromptOpen = true;
    try {
      await ElMessageBox.confirm(
        $t("common.mfaDialog.notEnabledMessage"),
        $t("common.mfaDialog.notEnabledTitle"),
        {
          type: "warning",
          confirmButtonText: $t("common.mfaDialog.goToSetup"),
          cancelButtonText: $t("common.cancel"),
          closeOnClickModal: false,
        },
      );
      await router.push(resolveMfaSetupPath(setupPath));
    } catch {
      // 用户取消：保持在当前页面。
    } finally {
      setupPromptOpen = false;
    }
  },
};
