import { $t } from "@/locales";
import { createToastGate, type ToastKind } from "./toast-gate";

export type ToastVariant = "success" | "error" | "warning" | "info";

export interface ToastPayload {
  variant: ToastVariant;
  message: string;
  duration?: number;
}

export type ToastSink = (payload: ToastPayload) => void;
export type ToastClearSink = (variant?: ToastVariant) => void;

const gate = createToastGate();
let toastSink: ToastSink | null = null;
let toastClearSink: ToastClearSink | null = null;
let installed = false;

/** Stage 2 UI shell binds its Toast container here; safe no-op before mount. */
export function registerToastSink(
  sink: ToastSink | null,
  clearSink?: ToastClearSink | null,
): void {
  toastSink = sink;
  toastClearSink = clearSink ?? null;
}

export function showToast(
  variant: ToastVariant,
  message: string,
  duration?: number,
): void {
  if (toastSink) {
    toastSink({ variant, message, duration });
  }
}

export function showSuccessToast(message: string, duration?: number): void {
  showToast("success", message, duration);
}

export function showWarningToast(message: string, duration?: number): void {
  showToast("warning", message, duration);
}

export function showInfoToast(message: string, duration?: number): void {
  showToast("info", message, duration);
}

/** HTTP 层统一错误出口：网络错误走全局冷却，其余按文案去重。 */
export function showErrorToast(
  message: string,
  kind: ToastKind = "error",
): void {
  if (!gate.admit(message, kind)) return;
  showToast("error", message);
}

/** 启动时安装一次：监听浏览器 offline / online 事件并统一提示。 */
export function installMessageGuard(): void {
  if (installed || typeof window === "undefined") return;
  installed = true;

  window.addEventListener("offline", () => {
    gate.setOffline(true);
    toastClearSink?.("error");
    showWarningToast($t("httpMsg.offline"));
  });
  window.addEventListener("online", () => {
    gate.setOffline(false);
    showSuccessToast($t("httpMsg.online"));
  });
  if (typeof navigator !== "undefined" && navigator.onLine === false) {
    gate.setOffline(true);
  }
}
