/**
 * MFA 提权窗口的纯函数。不依赖 Vue，供 node:test 与界面共用。
 * 截止时间只根据服务器 remaining_secs 推算，不比较客户端与服务器的绝对时钟。
 */

export const BACKUP_CODE_PATTERN = /^[A-HJ-NP-Z2-9]{8}$/;

export function toClientDeadline(remainingSecs: number, now = Date.now()): number {
  const secs = Number.isFinite(remainingSecs) ? Math.max(0, remainingSecs) : 0;
  return now + secs * 1000;
}

/** 超过 60 分钟仍按分钟累加，例如 3661 → "61:01"。 */
export function formatMmSs(secs: number): string {
  const total = Number.isFinite(secs) ? Math.max(0, Math.floor(secs)) : 0;
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

export function isWarning(secs: number): boolean {
  return Number.isFinite(secs) && secs > 0 && secs <= 120;
}

function pad2(value: string): string {
  return value.length >= 2 ? value : value.padStart(2, "0");
}

/**
 * 把 ISO 时间格式化成东八区墙上时钟。不用 utils/format.ts 的 formatDateTime（那是 ISO UTC）。
 * `2026-10-27T07:15:00Z` → `2026-10-27 15:15:00 (UTC+8)`。
 */
export function formatExpiryUtc8(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  const parts = new Intl.DateTimeFormat("en-US", {
    timeZone: "Asia/Shanghai",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hourCycle: "h23",
  }).formatToParts(date);
  const pick = (type: Intl.DateTimeFormatPartTypes): string =>
    parts.find((part) => part.type === type)?.value ?? "";
  const year = pick("year");
  const month = pad2(pick("month"));
  const day = pad2(pick("day"));
  let hour = pad2(pick("hour"));
  const minute = pad2(pick("minute"));
  const second = pad2(pick("second"));
  if (hour === "24") hour = "00";
  if (!year || !month || !day || hour === "" || !minute || !second) return "";
  return `${year}-${month}-${day} ${hour}:${minute}:${second} (UTC+8)`;
}

/** 只接受 `{type:"changed"}`。多余字段、字符串、其他 type、非对象都是 null。 */
export function parseBroadcast(raw: unknown): { type: "changed" } | null {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return null;
  const record = raw as Record<string, unknown>;
  const keys = Object.keys(record);
  if (keys.length !== 1 || keys[0] !== "type" || record.type !== "changed") {
    return null;
  }
  return { type: "changed" };
}

export type ExclusiveMfaCode = { totp_code: string } | { backup_code: string };

/**
 * 纯数字且长度等于 totpDigits → totp_code。
 * 否则按备用码字符集（自动大写）。二者不会同时返回。
 * totpDigits 为 8 时，全数字优先当作 TOTP。
 */
export function classifySubmittedCode(
  raw: string,
  totpDigits: number,
): ExclusiveMfaCode | null {
  const trimmed = raw.trim();
  if (!trimmed) return null;
  const digits = totpDigits === 8 ? 8 : 6;
  if (new RegExp(`^\\d{${digits}}$`).test(trimmed)) {
    return { totp_code: trimmed };
  }
  const backup = trimmed.toUpperCase();
  if (BACKUP_CODE_PATTERN.test(backup)) {
    return { backup_code: backup };
  }
  return null;
}

export interface SessionExpiryFields {
  expires_in?: number;
  remaining_secs?: number;
  expires_at?: string;
}

/**
 * 启用/关闭后重签会话的剩余秒数。优先服务器给出的 expires_in / remaining_secs，
 * 避免用绝对时钟；只有二者都没有时才从 expires_at 推算。
 */
export function readSessionExpiresIn(
  payload: SessionExpiryFields | null | undefined,
): number | null {
  if (!payload) return null;
  if (
    typeof payload.expires_in === "number" &&
    Number.isFinite(payload.expires_in) &&
    payload.expires_in > 0
  ) {
    return Math.floor(payload.expires_in);
  }
  if (
    typeof payload.remaining_secs === "number" &&
    Number.isFinite(payload.remaining_secs) &&
    payload.remaining_secs > 0
  ) {
    return Math.floor(payload.remaining_secs);
  }
  if (typeof payload.expires_at === "string" && payload.expires_at) {
    const ms = Date.parse(payload.expires_at);
    if (Number.isFinite(ms)) {
      const secs = Math.floor((ms - Date.now()) / 1000);
      return secs > 0 ? secs : null;
    }
  }
  return null;
}
