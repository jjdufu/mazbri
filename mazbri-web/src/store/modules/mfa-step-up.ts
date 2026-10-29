import { defineStore } from "pinia";
import { ref } from "vue";
import type {
  MfaStepUpEndResponse,
  MfaStepUpRequest,
  MfaStepUpResponse,
  MfaStepUpStatus,
} from "@/types/api";
import {
  MFA_CATEGORIES,
  emptyCategorySnapshot,
  isMfaCategory,
  parseBroadcast,
  toClientDeadline,
  type MfaCategory,
} from "@/utils/http/mfa-step-up-state";
import { ErrorCodes } from "@/constants/error-codes";
import { logger } from "@/utils/sys";

const CHANNEL_NAME = "mazbri-mfa-step-up";
const STORAGE_KEY = "mazbri-mfa-step-up";
const POLL_MS = 60_000;

interface CategoryWindow {
  active: boolean;
  clientDeadline: number | null;
  expiresAt: string | null;
  reason: string | null;
}

function normalizeDigits(value: number | undefined): 6 | 8 {
  return value === 8 ? 8 : 6;
}

function blankWindows(): Record<MfaCategory, CategoryWindow> {
  const windows = {} as Record<MfaCategory, CategoryWindow>;
  for (const item of emptyCategorySnapshot()) {
    windows[item.category] = {
      active: false,
      clientDeadline: null,
      expiresAt: null,
      reason: item.reason,
    };
  }
  return windows;
}

export const useMfaStepUpStore = defineStore("mfaStepUpStore", () => {
  const mfaEnabled = ref(false);
  const mfaRequired = ref(false);
  const totpDigits = ref<6 | 8>(6);
  const serverTime = ref<string | null>(null);
  const ttlSecs = ref(0);
  const loaded = ref(false);
  const nowMs = ref(Date.now());
  const windows = ref<Record<MfaCategory, CategoryWindow>>(blankWindows());

  let epoch = 0;
  let refreshInflight: Promise<void> | null = null;
  let pollTimer: ReturnType<typeof setInterval> | null = null;
  let expiryTimer: ReturnType<typeof setTimeout> | null = null;
  let tickTimer: ReturnType<typeof setInterval> | null = null;
  let expiryRefreshPending = false;
  let listenersBound = false;
  let channel: BroadcastChannel | null = null;

  function clearTimers(): void {
    if (pollTimer) clearInterval(pollTimer);
    if (expiryTimer) clearTimeout(expiryTimer);
    if (tickTimer) clearInterval(tickTimer);
    pollTimer = null;
    expiryTimer = null;
    tickTimer = null;
  }

  function remainingNow(category: MfaCategory, now = nowMs.value): number {
    const window = windows.value[category];
    if (!window.active || window.clientDeadline == null) return 0;
    return Math.max(0, Math.ceil((window.clientDeadline - now) / 1000));
  }

  function isCategoryActive(category: MfaCategory, now = Date.now()): boolean {
    return remainingNow(category, now) > 0;
  }

  function soonestDeadline(): number | null {
    let soonest: number | null = null;
    for (const category of MFA_CATEGORIES) {
      const window = windows.value[category];
      if (!window.active || window.clientDeadline == null) continue;
      if (soonest == null || window.clientDeadline < soonest) {
        soonest = window.clientDeadline;
      }
    }
    return soonest;
  }

  function scheduleTimers(): void {
    clearTimers();
    if (typeof window === "undefined") return;
    const soonest = soonestDeadline();
    if (soonest == null) return;
    tickTimer = setInterval(() => {
      nowMs.value = Date.now();
      if (MFA_CATEGORIES.some((category) => windows.value[category].active && remainingNow(category) <= 0)) {
        void onCountdownZero();
      }
    }, 1000);
    expiryTimer = setTimeout(() => {
      void onCountdownZero();
    }, Math.max(0, soonest - Date.now()) + 20);
    pollTimer = setInterval(() => {
      void refresh().catch(() => undefined);
    }, POLL_MS);
  }

  function applySnapshot(data: MfaStepUpStatus, now = Date.now()): void {
    mfaEnabled.value = Boolean(data.mfa_enabled);
    mfaRequired.value = Boolean(data.mfa_required);
    totpDigits.value = normalizeDigits(data.totp_digits);
    serverTime.value = data.server_time ?? null;
    ttlSecs.value = typeof data.ttl_secs === "number" ? data.ttl_secs : 0;
    loaded.value = true;
    const next = blankWindows();
    const seen = new Set<MfaCategory>();
    for (const item of data.categories ?? []) {
      if (!isMfaCategory(item.category) || seen.has(item.category)) continue;
      seen.add(item.category);
      const secs =
        typeof item.remaining_secs === "number"
          ? Math.max(0, item.remaining_secs)
          : 0;
      if (item.active && secs > 0) {
        next[item.category] = {
          active: true,
          clientDeadline: toClientDeadline(secs, now),
          expiresAt: item.expires_at,
          reason: item.reason,
        };
      } else {
        next[item.category] = {
          active: false,
          clientDeadline: null,
          expiresAt: null,
          reason: item.reason ?? "none",
        };
      }
    }
    windows.value = next;
    nowMs.value = now;
    scheduleTimers();
  }

  function applyOpened(
    category: MfaCategory,
    data: MfaStepUpResponse,
    now = Date.now(),
  ): void {
    const secs = Number.isFinite(data.remaining_secs)
      ? Math.max(0, data.remaining_secs)
      : 0;
    windows.value = {
      ...windows.value,
      [category]: {
        active: Boolean(data.active) && secs > 0,
        clientDeadline:
          data.active && secs > 0 ? toClientDeadline(secs, now) : null,
        expiresAt: data.expires_at ?? null,
        reason: null,
      },
    };
    if (typeof data.totp_digits === "number") {
      totpDigits.value = normalizeDigits(data.totp_digits);
    }
    if (typeof data.ttl_secs === "number") ttlSecs.value = data.ttl_secs;
    serverTime.value = data.server_time ?? serverTime.value;
    nowMs.value = now;
    scheduleTimers();
  }

  async function isLoggedIn(): Promise<boolean> {
    const { useUserStore } = await import("@/store/modules/user");
    return useUserStore().isLogin;
  }

  async function refresh(): Promise<void> {
    if (!(await isLoggedIn())) return;
    if (refreshInflight) return refreshInflight;
    const ticket = epoch;
    refreshInflight = (async () => {
      const { getMfaStepUp } = await import("@/api/auth");
      const data = await getMfaStepUp();
      if (ticket !== epoch) return;
      applySnapshot(data);
    })()
      .catch((error: unknown) => {
        logger.warn("[MFA] Failed to refresh step-up status", {
          service: "mazbri-mgmt-web",
          event: "mfa_step_up_refresh_failed",
          severity: "warn",
          error_code: ErrorCodes.INTERNAL_ERROR,
          error,
        });
        throw error;
      })
      .finally(() => {
        refreshInflight = null;
      });
    return refreshInflight;
  }

  async function onCountdownZero(): Promise<void> {
    if (expiryRefreshPending) return;
    expiryRefreshPending = true;
    clearTimers();
    try {
      await refresh();
    } catch {
      const now = Date.now();
      const next = { ...windows.value };
      for (const category of MFA_CATEGORIES) {
        const item = next[category];
        if (item.active && item.clientDeadline != null && item.clientDeadline <= now) {
          next[category] = {
            active: false,
            clientDeadline: null,
            expiresAt: null,
            reason: "expired",
          };
        }
      }
      windows.value = next;
      nowMs.value = now;
    } finally {
      expiryRefreshPending = false;
      scheduleTimers();
    }
  }

  function broadcastChanged(): void {
    const message = { type: "changed" as const };
    if (channel) {
      channel.postMessage(message);
      return;
    }
    if (typeof localStorage === "undefined") return;
    try {
      localStorage.removeItem(STORAGE_KEY);
      localStorage.setItem(STORAGE_KEY, JSON.stringify(message));
    } catch {
      // 隐私模式或配额不足时，本页状态仍然有效。
    }
  }

  async function stepUp(payload: MfaStepUpRequest): Promise<MfaStepUpResponse> {
    const ticket = epoch;
    const { stepUpMfa } = await import("@/api/auth");
    const data = await stepUpMfa(payload);
    if (ticket !== epoch) return data;
    const opened = isMfaCategory(data.category) ? data.category : payload.category;
    applyOpened(opened, data);
    broadcastChanged();
    return data;
  }

  function clearElevation(): void {
    windows.value = blankWindows();
    nowMs.value = Date.now();
    clearTimers();
  }

  function assertEndScope(data: MfaStepUpEndResponse, category?: MfaCategory): void {
    if (category) {
      if (data.scope !== "category" || data.category !== category) {
        throw new Error("unexpected category end response");
      }
      return;
    }
    if (data.scope !== "all") {
      throw new Error("unexpected step-up end response");
    }
  }

  async function lock(): Promise<void> {
    const ticket = epoch;
    const { endMfaStepUp } = await import("@/api/auth");
    const data = await endMfaStepUp();
    assertEndScope(data);
    if (ticket !== epoch) return;
    clearElevation();
    broadcastChanged();
  }

  async function endCategory(category: MfaCategory): Promise<void> {
    const ticket = epoch;
    const { endMfaStepUp } = await import("@/api/auth");
    const data = await endMfaStepUp(category);
    assertEndScope(data, category);
    if (ticket !== epoch) return;
    windows.value = {
      ...windows.value,
      [category]: {
        active: false,
        clientDeadline: null,
        expiresAt: null,
        reason: "none",
      },
    };
    nowMs.value = Date.now();
    scheduleTimers();
    broadcastChanged();
  }

  function reset(): void {
    epoch += 1;
    clearTimers();
    mfaEnabled.value = false;
    mfaRequired.value = false;
    totpDigits.value = 6;
    serverTime.value = null;
    ttlSecs.value = 0;
    loaded.value = false;
    clearElevation();
    broadcastChanged();
  }

  function onRemoteChanged(raw: unknown): void {
    if (!parseBroadcast(raw)) return;
    void refresh().catch(() => undefined);
  }

  function onStorage(event: StorageEvent): void {
    if (event.key !== STORAGE_KEY || typeof event.newValue !== "string") return;
    let parsed: unknown;
    try {
      parsed = JSON.parse(event.newValue) as unknown;
    } catch {
      return;
    }
    onRemoteChanged(parsed);
  }

  function onVisibility(): void {
    if (document.visibilityState === "visible") {
      void refresh().catch(() => undefined);
    }
  }

  function onFocus(): void {
    void refresh().catch(() => undefined);
  }

  function bindRuntime(): void {
    if (listenersBound || typeof window === "undefined") return;
    listenersBound = true;
    document.addEventListener("visibilitychange", onVisibility);
    window.addEventListener("focus", onFocus);
    try {
      if (typeof BroadcastChannel === "undefined") {
        throw new Error("BroadcastChannel unavailable");
      }
      channel = new BroadcastChannel(CHANNEL_NAME);
      channel.onmessage = (event: MessageEvent<unknown>) => {
        onRemoteChanged(event.data);
      };
    } catch {
      channel = null;
      window.addEventListener("storage", onStorage);
    }
  }

  bindRuntime();

  return {
    mfaEnabled,
    mfaRequired,
    totpDigits,
    serverTime,
    ttlSecs,
    loaded,
    nowMs,
    windows,
    refresh,
    stepUp,
    lock,
    endCategory,
    clearElevation,
    reset,
    remainingNow,
    isCategoryActive,
  };
});
