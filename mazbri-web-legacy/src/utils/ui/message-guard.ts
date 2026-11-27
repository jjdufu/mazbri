import { $t } from "@/locales";
import { createToastGate, type ToastKind } from "./toast-gate";

type MessageErrorFn = typeof ElMessage.error;

const gate = createToastGate();
let rawError: MessageErrorFn | null = null;

function readMessageText(options: unknown): string | undefined {
  if (typeof options === "string") return options;
  if (options && typeof options === "object" && "message" in options) {
    const message = (options as { message?: unknown }).message;
    if (typeof message === "string") return message;
  }
  return undefined;
}

/** HTTP 层统一错误出口：网络错误走全局冷却，其余按文案去重。 */
export function showErrorToast(
  message: string,
  kind: ToastKind = "error",
): void {
  if (!gate.admit(message, kind)) return;
  (rawError ?? ElMessage.error)(message);
}

/**
 * 启动时安装一次：让页面里散落的 ElMessage.error 也经过同一闸门，
 * 并在浏览器 offline / online 时只提示一次「网络已断开 / 已恢复」。
 */
export function installMessageGuard(): void {
  if (rawError) return;
  const original = ElMessage.error;
  rawError = original;
  ElMessage.error = ((options, appContext) => {
    const text = readMessageText(options);
    if (text !== undefined && !gate.admit(text)) {
      return { close: () => undefined };
    }
    return original(options, appContext);
  }) as MessageErrorFn;

  window.addEventListener("offline", () => {
    gate.setOffline(true);
    ElMessage.closeAll("error");
    ElMessage.warning($t("httpMsg.offline"));
  });
  window.addEventListener("online", () => {
    gate.setOffline(false);
    ElMessage.success($t("httpMsg.online"));
  });
  if (navigator.onLine === false) {
    gate.setOffline(true);
  }
}
