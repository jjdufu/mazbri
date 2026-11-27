import type { Router } from "vue-router";
import { ElMessageBox } from "element-plus";
import { $t } from "@/locales";
import {
  hasPromptedChunkHardRefresh,
  isChunkLoadError,
  markChunkHardRefreshPrompted,
} from "./chunk-load-error";

export {
  CHUNK_HARD_REFRESH_SESSION_KEY,
  hasPromptedChunkHardRefresh,
  isChunkLoadError,
  markChunkHardRefreshPrompted,
} from "./chunk-load-error";

let prompting = false;
let installed = false;

function safeSessionStorage(): Storage | null {
  try {
    return typeof sessionStorage !== "undefined" ? sessionStorage : null;
  } catch {
    return null;
  }
}

/**
 * 识别到 chunk / 动态 import 失败后：中文提示 → 用户确认再 location.reload。
 * 禁止静默 reload；同一会话只提示一次。
 */
export async function promptChunkHardRefresh(options?: {
  reload?: () => void;
  storage?: Pick<Storage, "getItem" | "setItem"> | null;
}): Promise<boolean> {
  const storage: Pick<Storage, "getItem" | "setItem"> | null =
    options && "storage" in options
      ? (options.storage ?? null)
      : safeSessionStorage();
  if (prompting || hasPromptedChunkHardRefresh(storage)) {
    return false;
  }
  prompting = true;
  // 打开对话框即记账，取消也不再连环弹窗；用户仍可用浏览器硬刷新。
  markChunkHardRefreshPrompted(storage);
  try {
    await ElMessageBox.confirm(
      $t("common.chunkHardRefresh.message"),
      $t("common.chunkHardRefresh.title"),
      {
        type: "warning",
        confirmButtonText: $t("common.chunkHardRefresh.confirm"),
        cancelButtonText: $t("common.cancel"),
        closeOnClickModal: false,
        closeOnPressEscape: false,
        showClose: false,
        distinguishCancelAndClose: true,
      },
    );
    const reload =
      options?.reload ??
      (() => {
        window.location.reload();
      });
    reload();
    return true;
  } catch {
    return false;
  } finally {
    prompting = false;
  }
}

/** 供单测重置模块内防抖状态。 */
export function resetChunkHardRefreshStateForTests(): void {
  prompting = false;
  installed = false;
}

/**
 * 安装 router.onError + window vite:preloadError。
 * 幂等：重复调用只挂一次。
 */
export function installChunkHardRefresh(router: Router): void {
  if (installed) return;
  installed = true;

  router.onError((error) => {
    if (!isChunkLoadError(error)) return;
    void promptChunkHardRefresh();
  });

  window.addEventListener("vite:preloadError", (event: Event) => {
    const preload = event as VitePreloadErrorEvent;
    // 阻止 Vite 默认再抛一次未处理异常导致白屏
    preload.preventDefault();
    void promptChunkHardRefresh();
  });
}
