import assert from "node:assert/strict";
import { test } from "node:test";
import { buildServiceDetailRoute } from "../services-link";

test("buildServiceDetailRoute encodes service id under /ops/services", () => {
  assert.deepEqual(buildServiceDetailRoute("mazbri-gateway"), {
    path: "/ops/services/mazbri-gateway",
  });
  assert.deepEqual(buildServiceDetailRoute("a/b"), {
    path: "/ops/services/a%2Fb",
  });
});
