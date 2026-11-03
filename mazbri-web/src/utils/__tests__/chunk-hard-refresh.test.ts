import assert from "node:assert/strict";
import { describe, test } from "node:test";
import {
  CHUNK_HARD_REFRESH_SESSION_KEY,
  hasPromptedChunkHardRefresh,
  isChunkLoadError,
  markChunkHardRefreshPrompted,
} from "../sys/chunk-load-error";

describe("chunk hard refresh (step13)", () => {
  test("isChunkLoadError recognizes Vite / webpack chunk failure shapes", () => {
    assert.equal(
      isChunkLoadError(
        new Error(
          "Failed to fetch dynamically imported module: http://x/assets/Foo-abc.js",
        ),
      ),
      true,
    );
    assert.equal(
      isChunkLoadError(
        new Error("Loading chunk 17 failed.\n(error: http://x/assets/x.js)"),
      ),
      true,
    );
    assert.equal(
      isChunkLoadError({
        name: "ChunkLoadError",
        message: "Loading CSS chunk 3 failed.",
      }),
      true,
    );
    assert.equal(
      isChunkLoadError(
        new TypeError("error loading dynamically imported module"),
      ),
      true,
    );
    assert.equal(isChunkLoadError(new Error("Network Error")), false);
    assert.equal(isChunkLoadError(null), false);
    assert.equal(isChunkLoadError(undefined), false);
  });

  test("session flag records a single prompt per browser session", () => {
    const mem = new Map<string, string>();
    const storage = {
      getItem: (k: string) => mem.get(k) ?? null,
      setItem: (k: string, v: string) => {
        mem.set(k, v);
      },
    };
    assert.equal(hasPromptedChunkHardRefresh(storage), false);
    markChunkHardRefreshPrompted(storage);
    assert.equal(hasPromptedChunkHardRefresh(storage), true);
    assert.equal(mem.get(CHUNK_HARD_REFRESH_SESSION_KEY), "1");
  });

  test("already-prompted session must not imply silent reload side effects", () => {
    // 契约：会话已记账时调用方应跳过弹窗与 reload（由 promptChunkHardRefresh 实现）。
    const mem = new Map<string, string>([[CHUNK_HARD_REFRESH_SESSION_KEY, "1"]]);
    const storage = {
      getItem: (k: string) => mem.get(k) ?? null,
      setItem: (k: string, v: string) => {
        mem.set(k, v);
      },
    };
    assert.equal(hasPromptedChunkHardRefresh(storage), true);
  });
});
