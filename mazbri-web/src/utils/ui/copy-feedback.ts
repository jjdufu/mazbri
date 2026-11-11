import { h } from "vue";
import { ElMessage, ElMessageBox } from "element-plus";
import { $t } from "@/locales";
import { copyTextToClipboard } from "@/utils/clipboard";

/**
 * 复制并给出统一反馈：成功轻提示；失败弹小框放可选中的原文，提示手动复制。
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
  showManualCopyDialog(value);
  return false;
}

function showManualCopyDialog(value: string): void {
  void ElMessageBox.alert(
    h("div", { class: "copy-manual" }, [
      h(
        "p",
        { style: "margin: 0 0 8px" },
        $t("common.feedback.copyManualHint"),
      ),
      h("textarea", {
        readonly: true,
        value,
        rows: Math.min(8, Math.max(2, value.split("\n").length)),
        style:
          "width: 100%; font-family: var(--el-font-family-mono, monospace); font-size: 12px; resize: vertical;",
        onVnodeMounted: (vnode: { el: unknown }) => {
          const el = vnode.el as HTMLTextAreaElement | null;
          el?.focus();
          el?.select();
        },
        onFocus: (e: FocusEvent) => (e.target as HTMLTextAreaElement).select(),
      }),
    ]),
    $t("common.feedback.copyFailed"),
    {
      confirmButtonText: $t("common.actions.close"),
      customClass: "copy-manual-box",
    },
  ).catch(() => undefined);
}
