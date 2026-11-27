import { ElMessage } from "element-plus";
import { $t } from "@/locales";
import { copyTextToClipboard } from "@/utils/clipboard";

/**
 * 复制并给出统一反馈：成功 / 失败都只是一条轻提示，不弹框。
 * 失败提示走 ElMessage.error，经 message-guard 闸门去重。
 * 所有复制按钮（CopyableText、代码块、MFA 密钥等）都走这里。
 */
export async function copyWithFeedback(value: string): Promise<boolean> {
  const ok = await copyTextToClipboard(value);
  if (ok) {
    ElMessage.success({
      message: $t("common.feedback.copiedToClipboard"),
      duration: 1200,
      grouping: true,
    });
    return true;
  }
  ElMessage.error({
    message: $t("common.feedback.copyFailed"),
    grouping: true,
  });
  return false;
}
