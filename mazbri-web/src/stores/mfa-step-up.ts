import { create } from "zustand";
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
import { logger } from "@/utils/sys/logger";

const CHANNEL_NAME = "mazbri-mfa-step-up";
const STORAGE_KEY = "mazbri-mfa-step-up";
const POLL_MS = 60_000;
const REFRESH_TTL_MS = 10_000;

export interface CategoryWindow {
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

let epoch = 0;
let refreshInflight: Promise<void> | null = null;
let lastFetchedAt = 0;
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

function assertEndScope(data: MfaStepUpEndResponse): void {
  if (data.scope !== "all") {
    throw new Error("unexpected step-up end response");
  }
}

export interface MfaStepUpState {
  mfaEnabled: boolean;
  mfaRequired: boolean;
  totpDigits: 6 | 8;
  serverTime: string | null;
  loaded: boolean;
  nowMs: number;
  windows: Record<MfaCategory, CategoryWindow>;
  remainingNow: (category: MfaCategory, now?: number) => number;
  isCategoryActive: (category: MfaCategory, now?: number) => boolean;
  refresh: (force?: boolean) => Promise<void>;
  stepUp: (payload: MfaStepUpRequest) => Promise<MfaStepUpResponse>;
  lock: () => Promise<void>;
  clearElevation: () => void;
  reset: () => void;
}

export const useMfaStepUpStore = create<MfaStepUpState>((set, get) => {
  function soonestDeadline(): number | null {
    let soonest: number | null = null;
    const windows = get().windows;
    for (const category of MFA_CATEGORIES) {
      const window = windows[category];
      if (!window.active || window.clientDeadline == null) continue;
      if (soonest == null || window.clientDeadline < soonest) {
        soonest = window.clientDeadline;
      }
    }
    return soonest;
  }

  async function onCountdownZero(): Promise<void> {
    if (expiryRefreshPending) return;
    expiryRefreshPending = true;
    clearTimers();
    try {
      await get().refresh(true);
    } catch {
      const now = Date.now();
      const next = { ...get().windows };
      for (const category of MFA_CATEGORIES) {
        const item = next[category];
        if (
          item.active &&
          item.clientDeadline != null &&
          item.clientDeadline <= now
        ) {
          next[category] = {
            active: false,
            clientDeadline: null,
            expiresAt: null,
            reason: "expired",
          };
        }
      }
      set({ windows: next, nowMs: now });
    } finally {
      expiryRefreshPending = false;
      scheduleTimers();
    }
  }

  function scheduleTimers(): void {
    clearTimers();
    if (typeof window === "undefined") return;
    const soonest = soonestDeadline();
    if (soonest == null) return;
    tickTimer = setInterval(() => {
      const now = Date.now();
      set({ nowMs: now });
      const current = get();
      if (
        MFA_CATEGORIES.some(
          (category) =>
            current.windows[category].active &&
            current.remainingNow(category, now) <= 0,
        )
      ) {
        void onCountdownZero();
      }
    }, 1000);
    expiryTimer = setTimeout(() => {
      void onCountdownZero();
    }, Math.max(0, soonest - Date.now()) + 20);
    pollTimer = setInterval(() => {
      void get()
        .refresh()
        .catch(() => undefined);
    }, POLL_MS);
  }

  function applySnapshot(data: MfaStepUpStatus, now = Date.now()): void {
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
    set({
      mfaEnabled: Boolean(data.mfa_enabled),
      mfaRequired: Boolean(data.mfa_required),
      totpDigits: normalizeDigits(data.totp_digits),
      serverTime: data.server_time ?? null,
      loaded: true,
      windows: next,
      nowMs: now,
    });
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
    const prev = get();
    set({
      windows: {
        ...prev.windows,
        [category]: {
          active: Boolean(data.active) && secs > 0,
          clientDeadline:
            data.active && secs > 0 ? toClientDeadline(secs, now) : null,
          expiresAt: data.expires_at ?? null,
          reason: null,
        },
      },
      totpDigits:
        typeof data.totp_digits === "number"
          ? normalizeDigits(data.totp_digits)
          : prev.totpDigits,
      serverTime: data.server_time ?? prev.serverTime,
      nowMs: now,
    });
    scheduleTimers();
  }

  async function isLoggedIn(): Promise<boolean> {
    const { useAuthStore } = await import("@/stores/auth");
    return useAuthStore.getState().isLogin;
  }

  function bindRuntime(): void {
    if (listenersBound || typeof window === "undefined") return;
    listenersBound = true;
    const onRemoteChanged = (raw: unknown) => {
      if (!parseBroadcast(raw)) return;
      void get()
        .refresh(true)
        .catch(() => undefined);
    };
    document.addEventListener("visibilitychange", () => {
      if (document.visibilityState === "visible") {
        void get()
          .refresh()
          .catch(() => undefined);
      }
    });
    window.addEventListener("focus", () => {
      void get()
        .refresh()
        .catch(() => undefined);
    });
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
      window.addEventListener("storage", (event: StorageEvent) => {
        if (event.key !== STORAGE_KEY || typeof event.newValue !== "string") {
          return;
        }
        try {
          onRemoteChanged(JSON.parse(event.newValue) as unknown);
        } catch {
          // Ignore malformed broadcast payload.
        }
      });
    }
  }

  bindRuntime();

  return {
    mfaEnabled: false,
    mfaRequired: false,
    totpDigits: 6,
    serverTime: null,
    loaded: false,
    nowMs: Date.now(),
    windows: blankWindows(),
    remainingNow: (category: MfaCategory, now = get().nowMs) => {
      const window = get().windows[category];
      if (!window.active || window.clientDeadline == null) return 0;
      return Math.max(0, Math.ceil((window.clientDeadline - now) / 1000));
    },
    isCategoryActive: (category: MfaCategory, now = Date.now()) => {
      return get().remainingNow(category, now) > 0;
    },
    refresh: (force = false) => {
      if (!force && refreshInflight) return refreshInflight;
      if (!force && get().loaded && Date.now() - lastFetchedAt < REFRESH_TTL_MS) {
        return Promise.resolve();
      }
      const ticket = epoch;
      refreshInflight = (async () => {
        if (!(await isLoggedIn())) return;
        const { getMfaStepUp } = await import("@/api/auth");
        const data = await getMfaStepUp();
        if (ticket !== epoch) return;
        applySnapshot(data);
        lastFetchedAt = Date.now();
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
    },
    stepUp: async (payload: MfaStepUpRequest) => {
      const ticket = epoch;
      const { stepUpMfa } = await import("@/api/auth");
      const data = await stepUpMfa(payload);
      if (ticket !== epoch) return data;
      const opened = isMfaCategory(data.category)
        ? data.category
        : payload.category;
      applyOpened(opened, data);
      broadcastChanged();
      return data;
    },
    lock: async () => {
      const ticket = epoch;
      const { endMfaStepUp } = await import("@/api/auth");
      const data = await endMfaStepUp();
      assertEndScope(data);
      if (ticket !== epoch) return;
      get().clearElevation();
      broadcastChanged();
    },
    clearElevation: () => {
      clearTimers();
      set({
        windows: blankWindows(),
        nowMs: Date.now(),
      });
    },
    reset: () => {
      epoch += 1;
      lastFetchedAt = 0;
      clearTimers();
      set({
        mfaEnabled: false,
        mfaRequired: false,
        totpDigits: 6,
        serverTime: null,
        loaded: false,
        windows: blankWindows(),
        nowMs: Date.now(),
      });
      broadcastChanged();
    },
  };
});
