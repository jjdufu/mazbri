import { defineStore } from "pinia";
import { ref } from "vue";
import type { MfaStepUpRequest, MfaStepUpResponse, MfaStepUpStatus } from "@/types/api";
import { toClientDeadline, parseBroadcast } from "@/utils/http/mfa-step-up-state";
import { ErrorCodes } from "@/constants/error-codes";
import { logger } from "@/utils/sys";

const CHANNEL_NAME = "mazbri-mfa-step-up";
const STORAGE_KEY = "mazbri-mfa-step-up";
const POLL_MS = 60_000;

function normalizeDigits(value: number | undefined): 6 | 8 {
  return value === 8 ? 8 : 6;
}

export const useMfaStepUpStore = defineStore("mfaStepUpStore", () => {
  const mfaEnabled = ref(false);
  const mfaRequired = ref(false);
  const totpDigits = ref<6 | 8>(6);
  const active = ref(false);
  const remainingSecs = ref(0);
  const expiresAt = ref<string | null>(null);
  const serverTime = ref<string | null>(null);
  const ttlSecs = ref(0);
  const reason = ref<string | null>(null);
  const loaded = ref(false);
  /** 由 remaining_secs 推出的客户端截止时间，倒计时用它减 Date.now()，不累减。 */
  const clientDeadline = ref<number | null>(null);

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

  function remainingNow(now = Date.now()): number {
    if (!active.value || clientDeadline.value == null) return 0;
    return Math.max(0, Math.ceil((clientDeadline.value - now) / 1000));
  }

  function syncRemaining(): void {
    const secs = remainingNow();
    remainingSecs.value = secs;
    if (active.value && clientDeadline.value != null && secs <= 0) {
      void onCountdownZero();
    }
  }

  function scheduleTimers(): void {
    clearTimers();
    if (typeof window === "undefined") return;
    if (!active.value || clientDeadline.value == null) return;
    tickTimer = setInterval(syncRemaining, 1000);
    const delay = Math.max(0, clientDeadline.value - Date.now());
    expiryTimer = setTimeout(() => {
      void onCountdownZero();
    }, delay + 20);
    pollTimer = setInterval(() => {
      void refresh().catch(() => undefined);
    }, POLL_MS);
  }

  function applySnapshot(data: MfaStepUpStatus, now = Date.now()): void {
    mfaEnabled.value = Boolean(data.mfa_enabled);
    mfaRequired.value = Boolean(data.mfa_required);
    totpDigits.value = normalizeDigits(data.totp_digits);
    expiresAt.value = data.expires_at ?? null;
    serverTime.value = data.server_time ?? null;
    ttlSecs.value = typeof data.ttl_secs === "number" ? data.ttl_secs : 0;
    reason.value = data.reason ?? null;
    loaded.value = true;
    const secs =
      typeof data.remaining_secs === "number"
        ? Math.max(0, data.remaining_secs)
        : 0;
    if (data.active && secs > 0) {
      active.value = true;
      remainingSecs.value = secs;
      clientDeadline.value = toClientDeadline(secs, now);
    } else {
      active.value = false;
      remainingSecs.value = 0;
      clientDeadline.value = null;
    }
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
    clientDeadline.value = null;
    remainingSecs.value = 0;
    clearTimers();
    try {
      await refresh();
    } catch {
      active.value = false;
      remainingSecs.value = 0;
      clientDeadline.value = null;
    } finally {
      expiryRefreshPending = false;
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
    applySnapshot({
      mfa_enabled: true,
      mfa_required: mfaRequired.value,
      totp_digits: data.totp_digits ?? totpDigits.value,
      active: data.active,
      remaining_secs: data.remaining_secs,
      expires_at: data.expires_at ?? null,
      server_time: data.server_time ?? null,
      ttl_secs: data.ttl_secs,
      reason: null,
    });
    broadcastChanged();
    return data;
  }

  /** 立刻停掉本地倒计时。锁定或刷新失败时胶囊也不能继续显示绿色。 */
  function clearElevation(): void {
    active.value = false;
    remainingSecs.value = 0;
    clientDeadline.value = null;
    expiresAt.value = null;
    reason.value = "invalidated";
    clearTimers();
  }

  async function lock(): Promise<void> {
    const ticket = epoch;
    const { endMfaStepUp } = await import("@/api/auth");
    await endMfaStepUp();
    if (ticket !== epoch) return;
    applySnapshot({
      mfa_enabled: mfaEnabled.value,
      mfa_required: mfaRequired.value,
      totp_digits: totpDigits.value,
      active: false,
      remaining_secs: 0,
      expires_at: null,
      server_time: serverTime.value,
      ttl_secs: ttlSecs.value,
      reason: "none",
    });
    broadcastChanged();
  }

  function reset(): void {
    epoch += 1;
    clearTimers();
    mfaEnabled.value = false;
    mfaRequired.value = false;
    totpDigits.value = 6;
    active.value = false;
    remainingSecs.value = 0;
    expiresAt.value = null;
    serverTime.value = null;
    ttlSecs.value = 0;
    reason.value = null;
    loaded.value = false;
    clientDeadline.value = null;
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
    active,
    remainingSecs,
    expiresAt,
    serverTime,
    ttlSecs,
    reason,
    loaded,
    clientDeadline,
    refresh,
    stepUp,
    lock,
    clearElevation,
    reset,
    remainingNow,
  };
});
