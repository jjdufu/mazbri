/**
 * 全站唯一的复制实现。
 *
 * - 安全上下文（https / localhost）且有 `navigator.clipboard` 时用异步剪贴板 API；
 * - 否则（例如内网 http 访问管理台）走隐藏 textarea + `document.execCommand("copy")`；
 * - 两者都失败返回 false，由调用方提示用户手动复制。
 *
 * 只复制调用方传入的完整值，不从 DOM 文本里取（列表里显示的往往是缩写）。
 */
export async function copyTextToClipboard(text: string): Promise<boolean> {
  const value = String(text ?? "")
    .replace(/\u00a0/g, " ")
    .trim();
  if (!value) return false;

  if (
    typeof window !== "undefined" &&
    window.isSecureContext &&
    typeof navigator !== "undefined" &&
    navigator.clipboard?.writeText
  ) {
    try {
      await navigator.clipboard.writeText(value);
      return true;
    } catch {
      // 权限被拒等情况：继续尝试 execCommand。
    }
  }
  return copyWithExecCommand(value);
}

function copyWithExecCommand(value: string): boolean {
  if (typeof document === "undefined" || !document.body) return false;
  const textarea = document.createElement("textarea");
  textarea.value = value;
  textarea.setAttribute("readonly", "");
  textarea.style.position = "fixed";
  textarea.style.left = "-9999px";
  textarea.style.top = "0";
  textarea.style.opacity = "0";
  document.body.appendChild(textarea);
  try {
    textarea.focus();
    textarea.select();
    return document.execCommand("copy");
  } catch {
    return false;
  } finally {
    document.body.removeChild(textarea);
  }
}
