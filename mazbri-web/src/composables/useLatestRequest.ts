import { getCurrentScope, onScopeDispose, ref, type Ref } from "vue";

export interface LatestRequestContext {
  readonly seq: number;
  readonly signal: AbortSignal;
  readonly isLatest: () => boolean;
}

export interface UseLatestRequestOptions {
  abortOnDispose?: boolean;
}

export interface UseLatestRequestReturn<TArgs extends unknown[], TResult> {
  loading: Ref<boolean>;
  run: (...args: TArgs) => Promise<TResult | undefined>;
  runSilent: (...args: TArgs) => Promise<TResult | undefined>;
  cancel: () => void;
  isLatest: (seq: number) => boolean;
}

export function isRequestAbortedError(error: unknown): boolean {
  if (!error || typeof error !== "object") return false;
  const candidate = error as {
    name?: string;
    code?: string | number;
    cancelled?: boolean;
  };
  if (candidate.cancelled === true) return true;
  if (candidate.code === "ERR_CANCELED") return true;
  if (candidate.name === "AbortError" || candidate.name === "CanceledError") {
    return true;
  }
  return false;
}

/**
 * 并发请求竞态守卫：
 * - 每次触发递增 `seq` 并自动 `abort()` 上一轮未完成请求的 `AbortController`
 * - 采用引用计数维护 `loading`，消除旧请求结束时提前将 `loading` 置 `false` 的闪烁
 * - 旧请求或已中止请求的结果/异常静默丢弃，仅最新一轮请求向调用方交付结果或抛出业务错误
 */
export function useLatestRequest<TArgs extends unknown[], TResult>(
  fn: (ctx: LatestRequestContext, ...args: TArgs) => Promise<TResult>,
  options: UseLatestRequestOptions = {},
): UseLatestRequestReturn<TArgs, TResult> {
  const { abortOnDispose = true } = options;
  const loading = ref(false);
  let currentSeq = 0;
  let activeController: AbortController | null = null;
  let activeLoadingCount = 0;

  function isLatest(seq: number): boolean {
    return seq > 0 && seq === currentSeq;
  }

  function cancel(): void {
    currentSeq += 1;
    if (activeController) {
      activeController.abort();
      activeController = null;
    }
  }

  async function execute(
    trackLoading: boolean,
    ...args: TArgs
  ): Promise<TResult | undefined> {
    const seq = ++currentSeq;
    if (activeController) {
      activeController.abort();
    }
    const controller = new AbortController();
    activeController = controller;

    if (trackLoading) {
      activeLoadingCount += 1;
      loading.value = activeLoadingCount > 0;
    }

    const ctx: LatestRequestContext = {
      seq,
      signal: controller.signal,
      isLatest: () => seq === currentSeq && !controller.signal.aborted,
    };

    try {
      const result = await fn(ctx, ...args);
      if (!ctx.isLatest()) {
        return undefined;
      }
      return result;
    } catch (error) {
      if (!ctx.isLatest() || isRequestAbortedError(error)) {
        return undefined;
      }
      throw error;
    } finally {
      if (activeController === controller) {
        activeController = null;
      }
      if (trackLoading) {
        activeLoadingCount = Math.max(0, activeLoadingCount - 1);
        loading.value = activeLoadingCount > 0;
      }
    }
  }

  function run(...args: TArgs): Promise<TResult | undefined> {
    return execute(true, ...args);
  }

  function runSilent(...args: TArgs): Promise<TResult | undefined> {
    return execute(false, ...args);
  }

  if (abortOnDispose && getCurrentScope()) {
    onScopeDispose(() => {
      cancel();
    });
  }

  return {
    loading,
    run,
    runSilent,
    cancel,
    isLatest,
  };
}
