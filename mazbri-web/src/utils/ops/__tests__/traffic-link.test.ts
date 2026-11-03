import assert from "node:assert/strict";
import { test } from "node:test";
import { TRAFFIC_PATH, buildTrafficRoute } from "../traffic-link";

test("buildTrafficRoute defaults to bare path", () => {
  assert.deepEqual(buildTrafficRoute(), { path: TRAFFIC_PATH, query: {} });
});

test("buildTrafficRoute with red tab", () => {
  assert.deepEqual(buildTrafficRoute({ tab: "red" }), {
    path: TRAFFIC_PATH,
    query: { tab: "red" },
  });
});

test("buildTrafficRoute with latency tab", () => {
  assert.deepEqual(buildTrafficRoute({ tab: "latency" }), {
    path: TRAFFIC_PATH,
    query: { tab: "latency" },
  });
});

test("buildTrafficRoute with errors tab", () => {
  assert.deepEqual(buildTrafficRoute({ tab: "errors" }), {
    path: TRAFFIC_PATH,
    query: { tab: "errors" },
  });
});
