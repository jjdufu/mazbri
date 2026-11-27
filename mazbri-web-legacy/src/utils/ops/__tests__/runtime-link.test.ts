import assert from "node:assert/strict";
import { test } from "node:test";
import { RUNTIME_PATH, buildRuntimeRoute } from "../runtime-link";

test("buildRuntimeRoute defaults to bare path", () => {
  assert.deepEqual(buildRuntimeRoute(), { path: RUNTIME_PATH, query: {} });
});

test("buildRuntimeRoute with tabs", () => {
  for (const tab of ["apps", "middleware", "pipeline", "hosts"] as const) {
    assert.deepEqual(buildRuntimeRoute({ tab }), {
      path: RUNTIME_PATH,
      query: { tab },
    });
  }
});
