import { $t } from "@/locales";
import { copyTextToClipboard } from "@/utils/clipboard";
import { showErrorToast, showSuccessToast } from "./message-guard";

/**
 * 复制并给出统一反馈：成功 / 失败都只是一条轻提示，不弹框。
 * 失败提示经 message-guard 闸门去重。
 */
export async function copyWithFeedback(value: string): Promise<boolean> {
  const ok = await copyTextToClipboard(value);
  if (ok) {
    showSuccessToast($t("common.feedback.copiedToClipboard"), 1200);
    return true;
  }
  showErrorToast($t("common.feedback.copyFailed"));
  return false;
}
