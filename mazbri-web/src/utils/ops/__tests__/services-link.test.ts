import assert from "node:assert/strict";
import { test } from "node:test";
import {
  SERVICES_PATH,
  buildServiceDetailRoute,
  buildServicesRoute,
} from "../services-link";

test("buildServicesRoute defaults to bare path", () => {
  assert.deepEqual(buildServicesRoute(), { path: SERVICES_PATH, query: {} });
});

test("buildServicesRoute with health tab", () => {
  assert.deepEqual(buildServicesRoute({ tab: "health" }), {
    path: SERVICES_PATH,
    query: { tab: "health" },
  });
});

test("buildServicesRoute with topology tab", () => {
  assert.deepEqual(buildServicesRoute({ tab: "topology" }), {
    path: SERVICES_PATH,
    query: { tab: "topology" },
  });
});

test("buildServiceDetailRoute encodes service id", () => {
  assert.deepEqual(buildServiceDetailRoute("mazbri-gateway"), {
    path: "/ops/services/mazbri-gateway",
  });
  assert.deepEqual(buildServiceDetailRoute("a/b"), {
    path: "/ops/services/a%2Fb",
  });
});
