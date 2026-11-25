import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { effectScope } from "vue";
import { useLatestRequest } from "../useLatestRequest";

function createDeferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe("useLatestRequest composable", () => {
  it("aborts superseded requests, keeps loading true until latest finishes, and drops stale results", async () => {
    const first = createDeferred<string>();
    const second = createDeferred<string>();
    const signals: AbortSignal[] = [];

    const latest = useLatestRequest(async (ctx, id: number) => {
      signals.push(ctx.signal);
      return id === 1 ? first.promise : second.promise;
    });

    assert.equal(latest.loading.value, false);

    const p1 = latest.run(1);
    assert.equal(latest.loading.value, true);
    assert.equal(signals[0]?.aborted, false);

    const p2 = latest.run(2);
    assert.equal(signals[0]?.aborted, true);
    assert.equal(signals[1]?.aborted, false);
    assert.equal(latest.loading.value, true);

    // Resolving the stale first request must return undefined and keep loading=true
    first.resolve("stale-1");
    const r1 = await p1;
    assert.equal(r1, undefined);
    assert.equal(latest.loading.value, true);

    // Resolving the latest request delivers its value and sets loading=false
    second.resolve("fresh-2");
    const r2 = await p2;
    assert.equal(r2, "fresh-2");
    assert.equal(latest.loading.value, false);
  });

  it("swallows AbortError / ERR_CANCELED while rethrowing business errors on the latest request", async () => {
    let mode: "abort" | "fail" = "abort";
    const latest = useLatestRequest(async () => {
      if (mode === "abort") {
        const err = new Error("canceled");
        (err as Error & { code?: string }).code = "ERR_CANCELED";
        throw err;
      }
      throw new Error("boom");
    });

    const abortedResult = await latest.run();
    assert.equal(abortedResult, undefined);
    assert.equal(latest.loading.value, false);

    mode = "fail";
    await assert.rejects(() => latest.run(), /boom/);
    assert.equal(latest.loading.value, false);
  });

  it("cancels active request automatically when enclosing Vue effectScope is disposed", async () => {
    const pending = createDeferred<string>();
    let capturedSignal: AbortSignal | null = null;

    const scope = effectScope();
    const runner = scope.run(() =>
      useLatestRequest(async (ctx) => {
        capturedSignal = ctx.signal;
        return pending.promise;
      }),
    )!;

    const promise = runner.run();
    assert.equal(capturedSignal!.aborted, false);

    scope.stop();
    assert.equal(capturedSignal!.aborted, true);

    pending.resolve("after-dispose");
    assert.equal(await promise, undefined);
  });
});
