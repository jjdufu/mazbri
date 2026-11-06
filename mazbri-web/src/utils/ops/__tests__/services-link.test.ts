import assert from "node:assert/strict";
import { test } from "node:test";
import {
  SERVICES_PATH,
  buildServiceDetailRoute,
  buildServicesRoute,
} from "../services-link";
import { RUNTIME_PATH } from "../runtime-link";

test("SERVICES_PATH aliases RUNTIME_PATH", () => {
  assert.equal(SERVICES_PATH, RUNTIME_PATH);
  assert.equal(SERVICES_PATH, "/ops/runtime");
});

test("buildServicesRoute always lands on runtime apps", () => {
  assert.deepEqual(buildServicesRoute(), {
    path: RUNTIME_PATH,
    query: { tab: "apps" },
  });
  assert.deepEqual(buildServicesRoute({ tab: "health" }), {
    path: RUNTIME_PATH,
    query: { tab: "apps" },
  });
  assert.deepEqual(buildServicesRoute({ tab: "apps" }), {
    path: RUNTIME_PATH,
    query: { tab: "apps" },
  });
});

test("buildServiceDetailRoute encodes service id under /ops/services", () => {
  assert.deepEqual(buildServiceDetailRoute("mazbri-gateway"), {
    path: "/ops/services/mazbri-gateway",
  });
  assert.deepEqual(buildServiceDetailRoute("a/b"), {
    path: "/ops/services/a%2Fb",
  });
});
