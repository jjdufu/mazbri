/**
 * MFA 守卫错误分类与弹窗队列决策。入参与 axios 错误上的响应体一致，不依赖 Vue。
 */

import { isMfaCategory, type MfaCategory } from "./mfa-step-up-state";

export interface MfaCategoryClassification {
  kind: "category";
  category: MfaCategory;
  reason: string | null;
}

export interface MfaNotEnabledClassification {
  kind: "not_enabled";
  setupPath: string | null;
}

export type MfaErrorClassification =
  | MfaCategoryClassification
  | MfaNotEnabledClassification;

export interface MfaInterceptConfig {
  url?: string;
  _mfaRetry?: boolean;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === "object" && !Array.isArray(value);
}

function httpStatus(record: Record<string, unknown>): number | null {
  if (typeof record.status === "number" && Number.isFinite(record.status)) {
    return record.status;
  }
  if (record.status === "503") return 503;
  return null;
}

/**
 * 只认带合法 category 的 MFA_REQUIRED。
 * 缺 category、旧 fresh 码、锁定和 503 都返回 null，走普通错误。
 */
export function classifyMfaError(body: unknown): MfaErrorClassification | null {
  if (!isRecord(body)) return null;
  if (httpStatus(body) === 503) return null;

  let record = body;
  if (typeof body.code === "number" && isRecord(body.data)) {
    if (body.code === 503) return null;
    record = body.data;
    if (httpStatus(record) === 503) return null;
  }

  const code = record.code;
  if (code === "MFA_LOCKED" || code === "MFA_CHECK_UNAVAILABLE") return null;
  if (code === "MFA_FRESH_REQUIRED") return null;
  if (code === "MFA_REQUIRED") {
    if (!isMfaCategory(record.category)) return null;
    return {
      kind: "category",
      category: record.category,
      reason: typeof record.reason === "string" ? record.reason : null,
    };
  }
  if (code === "MFA_NOT_ENABLED") {
    return {
      kind: "not_enabled",
      setupPath:
        typeof record.setup_path === "string" ? record.setup_path : null,
    };
  }
  return null;
}

function isStepUpRequest(url: unknown): boolean {
  return typeof url === "string" && url.includes("/auth/mfa/step-up");
}

/** 已用 _mfaRetry 重试过，或请求本身就是 step-up，则不再拦截。 */
export function shouldInterceptCategory(
  config: MfaInterceptConfig | null | undefined,
): boolean {
  if (!config) return false;
  if (config._mfaRetry) return false;
  if (isStepUpRequest(config.url)) return false;
  return true;
}

/** type 含 json 时 JSON.parse(await data.text())；否则 null。解析失败也返回 null。 */
export async function parseJsonBlob(data: Blob): Promise<unknown | null> {
  if (!data.type.toLowerCase().includes("json")) return null;
  try {
    return JSON.parse(await data.text()) as unknown;
  } catch {
    return null;
  }
}

/** 后来的类别排在后面，不插队，也不取消前面的任务。 */
export function insertDialogTask<T>(pending: T[], task: T): void {
  pending.push(task);
}

export async function drainDialogQueue<
  T extends {
    resolve: () => void;
    reject: (error: unknown) => void;
  },
>(pending: T[], run: (task: T) => Promise<void>): Promise<void> {
  while (pending.length > 0) {
    const task = pending.shift();
    if (!task) break;
    try {
      await run(task);
      task.resolve();
    } catch (error) {
      task.reject(error);
    }
  }
}
