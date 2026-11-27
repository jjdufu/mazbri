import { $t } from "@/locales";
import { ApiStatus } from "./status";
import { createSafeHttpError } from "./error";
import { drainDialogQueue, insertDialogTask } from "./mfa-error";
import {
  classifySubmittedCode,
  type ExclusiveMfaCode,
  type MfaCategory,
} from "./mfa-step-up-state";
import { MFA_SETUP_PATH, resolveMfaSetupPath } from "./mfa-setup-path";
import {
  MfaStepUpCancelledError,
  isMfaStepUpCancelled,
} from "./mfa-cancelled";

export { MFA_SETUP_PATH, MfaStepUpCancelledError, isMfaStepUpCancelled };

export interface MfaDialogViewModel {
  visible: boolean;
  sessionId: number;
  category: MfaCategory | "";
  totpDigits: 6 | 8;
  errorCode: string | null;
  attemptsRemaining: number | null;
  submitting: boolean;
}

export interface MfaSetupPromptRequest {
  resolvedPath: string;
  openPersonalSecurityDrawer: boolean;
}

export type MfaSetupPromptHandler = (
  request: MfaSetupPromptRequest,
) => Promise<void> | void;

type QueueTask = {
  category: MfaCategory;
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
  category: "",
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
const categoryWaiters = new Map<MfaCategory, Promise<void>>();
let setupPromptOpen = false;
let setupPromptHandler: MfaSetupPromptHandler | null = null;

function publish(): void {
  for (const listener of listeners) listener();
}

export function readMfaDialogView(): MfaDialogViewModel {
  return { ...viewState };
}

/** 全局只挂一个弹窗。React MfaStepUpHost 挂载时登记，卸载时解除。 */
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

/** 注册未绑定 MFA 时的引导确认处理桥（由 Stage 2 MfaStepUpHost 挂载）。 */
export function bindMfaSetupPrompt(
  handler: MfaSetupPromptHandler | null,
): () => void {
  setupPromptHandler = handler;
  return () => {
    if (setupPromptHandler === handler) {
      setupPromptHandler = null;
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
  viewState.category = task.category;
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
  // U1 公理：用户取消提权抛出独立 MfaStepUpCancelledError，与网络取消信号严格正交。
  decision?.reject(new MfaStepUpCancelledError());
}

async function execute(task: QueueTask): Promise<void> {
  const { useMfaStepUpStore } = await import("@/stores/mfa-step-up");
  const store = useMfaStepUpStore.getState();
  let refreshed = false;
  try {
    await store.refresh();
    refreshed = true;
  } catch {
    // 刷新失败不能沿用旧的 active。位数可能仍是上次的值；弹窗仍可提交。
  }
  if (refreshed && useMfaStepUpStore.getState().isCategoryActive(task.category)) {
    return;
  }
  const digits: 6 | 8 =
    useMfaStepUpStore.getState().totpDigits === 8 ? 8 : 6;
  await waitForHost();
  beginSession(task, digits);
  try {
    while (true) {
      const code = await waitForDecision();
      try {
        await useMfaStepUpStore
          .getState()
          .stepUp({ category: task.category, ...code });
        return;
      } catch (error) {
        if (readErrorCode(error) === "MFA_LOCKED") {
          const latest = useMfaStepUpStore.getState();
          latest.clearElevation();
          try {
            await latest.lock();
          } catch {
            await latest.refresh().catch(() => undefined);
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

export function ensureCategory(category: MfaCategory): Promise<void> {
  const existing = categoryWaiters.get(category);
  if (existing) return existing;
  const promise = enqueue({ category }).finally(() => {
    if (categoryWaiters.get(category) === promise) {
      categoryWaiters.delete(category);
    }
  });
  categoryWaiters.set(category, promise);
  return promise;
}

export async function ensureMfaBeforeOpen(
  category: MfaCategory,
): Promise<boolean> {
  try {
    await ensureCategory(category);
    return true;
  } catch (error: unknown) {
    if (isMfaStepUpCancelled(error)) {
      return false;
    }
    throw error;
  }
}

export async function promptSetup(setupPath: unknown): Promise<void> {
  if (setupPromptOpen) return;
  setupPromptOpen = true;
  try {
    const resolvedPath = resolveMfaSetupPath(setupPath);
    if (setupPromptHandler) {
      await setupPromptHandler({
        resolvedPath,
        openPersonalSecurityDrawer: resolvedPath === MFA_SETUP_PATH,
      });
    }
  } catch {
    // 用户取消：保持在当前页面。
  } finally {
    setupPromptOpen = false;
  }
}

export const MfaDialogService = {
  ensureCategory,
  promptSetup,
};
