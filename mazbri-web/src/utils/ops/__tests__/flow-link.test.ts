import assert from "node:assert/strict";
import { test } from "node:test";
import { FLOW_PATH, buildFlowRoute } from "../flow-link";

test("buildFlowRoute defaults to bare path", () => {
  assert.deepEqual(buildFlowRoute(), { path: FLOW_PATH, query: {} });
});

test("buildFlowRoute with ratelimit tab", () => {
  assert.deepEqual(buildFlowRoute({ tab: "ratelimit" }), {
    path: FLOW_PATH,
    query: { tab: "ratelimit" },
  });
});

test("buildFlowRoute with cache tab", () => {
  assert.deepEqual(buildFlowRoute({ tab: "cache" }), {
    path: FLOW_PATH,
    query: { tab: "cache" },
  });
});
