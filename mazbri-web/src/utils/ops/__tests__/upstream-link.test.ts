import assert from "node:assert/strict";
import { test } from "node:test";
import { UPSTREAM_PATH, buildUpstreamRoute } from "../upstream-link";

test("buildUpstreamRoute defaults to bare path", () => {
  assert.deepEqual(buildUpstreamRoute(), { path: UPSTREAM_PATH, query: {} });
});

test("buildUpstreamRoute with sla tab", () => {
  assert.deepEqual(buildUpstreamRoute({ tab: "sla" }), {
    path: UPSTREAM_PATH,
    query: { tab: "sla" },
  });
});

test("buildUpstreamRoute with breaker tab", () => {
  assert.deepEqual(buildUpstreamRoute({ tab: "breaker" }), {
    path: UPSTREAM_PATH,
    query: { tab: "breaker" },
  });
});
