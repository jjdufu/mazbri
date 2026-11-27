/**
 * 错误提示闸门（纯逻辑，不依赖 DOM / Element Plus，便于单测）。
 *
 * - 网络类错误（无响应、超时、断网）在冷却窗口内全局只放行一条；
 *   冷却期间或浏览器处于离线状态时，其余错误提示视为网络故障的连带结果，一律不放行。
 * - 普通错误按文案去重：同一文案在去重窗口内只放行一条。
 * - 网络恢复（online 事件）后立即清空冷却，下一次网络错误可以再次提示。
 */
export type ToastKind = "network" | "error";

export interface ToastGateOptions {
  now?: () => number;
  /** 同一普通错误文案的去重窗口 */
  dedupeWindowMs?: number;
  /** 网络错误冷却窗口 */
  networkCooldownMs?: number;
}

export interface ToastGate {
  admit(message: string, kind?: ToastKind): boolean;
  setOffline(offline: boolean): void;
  isOffline(): boolean;
}

export const TOAST_DEDUPE_WINDOW_MS = 3000;
export const NETWORK_TOAST_COOLDOWN_MS = 5000;

export function createToastGate(options: ToastGateOptions = {}): ToastGate {
  const now = options.now ?? Date.now;
  const dedupeWindowMs = options.dedupeWindowMs ?? TOAST_DEDUPE_WINDOW_MS;
  const networkCooldownMs =
    options.networkCooldownMs ?? NETWORK_TOAST_COOLDOWN_MS;
  const lastShownAt = new Map<string, number>();
  let networkMutedUntil = 0;
  let offline = false;

  return {
    admit(message, kind = "error") {
      const t = now();
      if (offline || t < networkMutedUntil) {
        return false;
      }
      if (kind === "network") {
        networkMutedUntil = t + networkCooldownMs;
        return true;
      }
      for (const [text, at] of lastShownAt) {
        if (t - at >= dedupeWindowMs) lastShownAt.delete(text);
      }
      if (lastShownAt.has(message)) {
        return false;
      }
      lastShownAt.set(message, t);
      return true;
    },
    setOffline(next) {
      offline = next;
      if (!next) {
        networkMutedUntil = 0;
      }
    },
    isOffline() {
      return offline;
    },
  };
}
