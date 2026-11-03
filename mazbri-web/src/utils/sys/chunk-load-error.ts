/** 同一浏览器会话内只弹一次，避免发版后多 chunk 连续 404 刷屏 / 连环 reload。 */
export const CHUNK_HARD_REFRESH_SESSION_KEY = "mgmt.chunkHardRefresh.prompted";

const CHUNK_ERROR_RE =
  /ChunkLoadError|Loading CSS chunk|Loading chunk [\w.-]+ failed|Failed to fetch dynamically imported module|error loading dynamically imported module|Importing a module script failed/i;

export function isChunkLoadError(err: unknown): boolean {
  if (err == null) return false;
  if (typeof err === "string") return CHUNK_ERROR_RE.test(err);
  if (typeof Event !== "undefined" && err instanceof Event) {
    const payload = (err as Event & { payload?: unknown }).payload;
    if (payload != null && isChunkLoadError(payload)) return true;
    // vite:preloadError 本身即动态 import 失败信号
    if (err.type === "vite:preloadError") return true;
  }
  if (typeof err === "object") {
    const anyErr = err as {
      name?: unknown;
      message?: unknown;
      stack?: unknown;
      toString?: () => string;
    };
    const parts = [
      typeof anyErr.name === "string" ? anyErr.name : "",
      typeof anyErr.message === "string" ? anyErr.message : "",
      typeof anyErr.stack === "string" ? anyErr.stack : "",
      typeof anyErr.toString === "function" ? String(anyErr.toString()) : "",
    ]
      .filter(Boolean)
      .join("\n");
    return CHUNK_ERROR_RE.test(parts);
  }
  return false;
}

export function hasPromptedChunkHardRefresh(
  storage: Pick<Storage, "getItem"> | null,
): boolean {
  if (!storage) return false;
  try {
    return storage.getItem(CHUNK_HARD_REFRESH_SESSION_KEY) === "1";
  } catch {
    return false;
  }
}

export function markChunkHardRefreshPrompted(
  storage: Pick<Storage, "setItem"> | null,
): void {
  if (!storage) return;
  try {
    storage.setItem(CHUNK_HARD_REFRESH_SESSION_KEY, "1");
  } catch {
    // sessionStorage 不可用时由调用方内存 flag 防抖
  }
}
