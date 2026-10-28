import { ElMessageBox } from "element-plus";
import { $t } from "@/locales";
import { router } from "@/router";
import { ApiStatus } from "./status";
import { createSafeHttpError } from "./error";
import { drainDialogQueue, insertDialogTask } from "./mfa-error";
import { classifySubmittedCode, type ExclusiveMfaCode } from "./mfa-step-up-state";
import { ACCOUNT_SECURITY_PATH, resolveMfaSetupPath } from "./mfa-setup-path";

export { ACCOUNT_SECURITY_PATH };

export class MfaStepUpCancelledError extends Error {
  readonly cancelled = true;

  constructor() {
    super("MFA step-up cancelled");
    this.name = "MfaStepUpCancelledError";
  }
}

export function isMfaStepUpCancelled(error: unknown): boolean {
  return error instanceof MfaStepUpCancelledError;
}

export interface MfaDialogViewModel {
  visible: boolean;
  sessionId: number;
  mode: "step_up" | "fresh";
  purpose: string;
  totpDigits: 6 | 8;
  errorCode: string | null;
  attemptsRemaining: number | null;
  submitting: boolean;
}

type QueueTask = {
  kind: "step_up" | "fresh";
  purpose?: string;
  resolve: () => void;
  reject: (error: unknown) => void;
};

type Decision = {
  resolve: (code: ExclusiveMfaCode) => void;
  reject: (error: unknown) => void;
};

const viewState: MfaDialogViewModel = {
  visible: false,
  sessionId: 0,
  mode: "step_up",
  purpose: "",
  totpDigits: 6,
  errorCode: null,
  attemptsRemaining: null,
  submitting: false,
};

const listeners = new Set<() => void>();
let hostBound = false;
const hostWaiters: Array<() => void> = [];
let activeDecision: Decision | null = null;
let sessionSerial = 0;

const queue: QueueTask[] = [];
let pumping = false;
let stepUpWaiter: Promise<void> | null = null;
const freshWaiters = new Map<string, Promise<void>>();
let setupPromptOpen = false;

function publish(): void {
  for (const listener of listeners) listener();
}

export function readMfaDialogView(): MfaDialogViewModel {
  return { ...viewState };
}

/** 全局只挂一个弹窗。组件挂载时登记，卸载时解除。 */
export function bindMfaStepUpDialog(listener: () => void): () => void {
  listeners.add(listener);
  hostBound = true;
  const waiters = hostWaiters.splice(0);
  for (const waiter of waiters) waiter();
  listener();
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0) {
      hostBound = false;
      cancelMfaDialog(viewState.sessionId);
    }
  };
}

function waitForHost(): Promise<void> {
  if (hostBound) return Promise.resolve();
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      reject(
        createSafeHttpError($t("common.mfaDialog.unavailable"), ApiStatus.error),
      );
    }, 15000);
    hostWaiters.push(() => {
      clearTimeout(timer);
      resolve();
    });
  });
}

function readBody(error: unknown): Record<string, unknown> | null {
  if (!error || typeof error !== "object") return null;
  const data = (error as { data?: unknown }).data;
  if (data && typeof data === "object" && !Array.isArray(data)) {
    return data as Record<string, unknown>;
  }
  return null;
}

function readErrorCode(error: unknown): string | null {
  const code = readBody(error)?.code;
  return typeof code === "string" ? code : null;
}

function readAttempts(error: unknown): number | null {
  const value = readBody(error)?.attempts_remaining;
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function isRecoverable(error: unknown): boolean {
  const code = readErrorCode(error);
  return (
    code === "MFA_INVALID_CODE" ||
    code === "MFA_CODE_REUSED" ||
    code === "MFA_INVALID_REQUEST"
  );
}

function beginSession(task: QueueTask, digits: 6 | 8): void {
  sessionSerial += 1;
  viewState.visible = true;
  viewState.sessionId = sessionSerial;
  viewState.mode = task.kind;
  viewState.purpose = task.purpose ?? "";
  viewState.totpDigits = digits;
  viewState.errorCode = null;
  viewState.attemptsRemaining = null;
  viewState.submitting = false;
  publish();
}

function endSession(): void {
  activeDecision = null;
  viewState.visible = false;
  viewState.submitting = false;
  publish();
}

function waitForDecision(): Promise<ExclusiveMfaCode> {
  return new Promise((resolve, reject) => {
    activeDecision = { resolve, reject };
  });
}

export function submitMfaDialog(raw: string): void {
  if (!activeDecision || viewState.submitting) return;
  const code = classifySubmittedCode(raw, viewState.totpDigits);
  if (!code) {
    viewState.errorCode = "invalid_format";
    viewState.attemptsRemaining = null;
    publish();
    return;
  }
  const decision = activeDecision;
  activeDecision = null;
  viewState.errorCode = null;
  viewState.submitting = true;
  publish();
  decision.resolve(code);
}

export function cancelMfaDialog(sessionId?: number): void {
  if (sessionId != null && sessionId !== viewState.sessionId) return;
  const decision = activeDecision;
  activeDecision = null;
  viewState.visible = false;
  viewState.submitting = false;
  publish();
  decision?.reject(new MfaStepUpCancelledError());
}

async function execute(task: QueueTask): Promise<void> {
  const { useMfaStepUpStore } = await import("@/store/modules/mfa-step-up");
  const store = useMfaStepUpStore();
  let refreshed = false;
  try {
    await store.refresh();
    refreshed = true;
  } catch {
    // 刷新失败不能沿用旧的 active。位数可能仍是上次的值；弹窗仍可提交。
  }
  if (task.kind === "step_up" && refreshed && store.active) return;
  const digits: 6 | 8 = store.totpDigits === 8 ? 8 : 6;
  await waitForHost();
  beginSession(task, digits);
  try {
    while (true) {
      const code = await waitForDecision();
      try {
        await store.stepUp(
          task.kind === "fresh" && task.purpose
            ? { ...code, purpose: task.purpose }
            : code,
        );
        return;
      } catch (error) {
        if (readErrorCode(error) === "MFA_LOCKED") {
          store.clearElevation();
          try {
            await store.lock();
          } catch {
            await store.refresh().catch(() => undefined);
          }
          throw createSafeHttpError(
            $t("common.mfaDialog.locked"),
            ApiStatus.forbidden,
          );
        }
        if (!isRecoverable(error)) throw error;
        viewState.submitting = false;
        viewState.errorCode = readErrorCode(error);
        viewState.attemptsRemaining = readAttempts(error);
        publish();
      }
    }
  } finally {
    endSession();
  }
}

async function pump(): Promise<void> {
  if (pumping) return;
  pumping = true;
  try {
    await drainDialogQueue(queue, execute);
  } finally {
    pumping = false;
    if (queue.length > 0) void pump();
  }
}

function enqueue(task: Omit<QueueTask, "resolve" | "reject">): Promise<void> {
  return new Promise((resolve, reject) => {
    insertDialogTask(queue, { ...task, resolve, reject });
    void pump();
  });
}

export function ensureStepUp(): Promise<void> {
  if (stepUpWaiter) return stepUpWaiter;
  const promise = enqueue({ kind: "step_up" }).finally(() => {
    if (stepUpWaiter === promise) stepUpWaiter = null;
  });
  stepUpWaiter = promise;
  return promise;
}

export function ensureFresh(purpose: string): Promise<void> {
  const existing = freshWaiters.get(purpose);
  if (existing) return existing;
  const promise = enqueue({ kind: "fresh", purpose }).finally(() => {
    if (freshWaiters.get(purpose) === promise) freshWaiters.delete(purpose);
  });
  freshWaiters.set(purpose, promise);
  return promise;
}

export async function promptSetup(setupPath: unknown): Promise<void> {
  if (setupPromptOpen) return;
  setupPromptOpen = true;
  try {
    await ElMessageBox.confirm(
      $t("common.mfaDialog.notEnabledMessage"),
      $t("common.mfaDialog.notEnabledTitle"),
      {
        type: "warning",
        confirmButtonText: $t("common.mfaDialog.goToSetup"),
        cancelButtonText: $t("common.cancel"),
        closeOnClickModal: false,
      },
    );
    await router.push(resolveMfaSetupPath(setupPath));
  } catch {
    // 用户取消：保持在当前页面。
  } finally {
    setupPromptOpen = false;
  }
}

export const MfaDialogService = {
  ensureStepUp,
  ensureFresh,
  promptSetup,
};
