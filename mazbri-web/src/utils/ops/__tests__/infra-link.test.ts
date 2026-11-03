import assert from "node:assert/strict";
import { test } from "node:test";
import { INFRA_PATH, buildInfraRoute } from "../infra-link";

test("buildInfraRoute defaults to bare path", () => {
  assert.deepEqual(buildInfraRoute(), { path: INFRA_PATH, query: {} });
});

test("buildInfraRoute with tabs", () => {
  assert.deepEqual(buildInfraRoute({ tab: "pipeline" }), {
    path: INFRA_PATH,
    query: { tab: "pipeline" },
  });
  assert.deepEqual(buildInfraRoute({ tab: "middleware" }), {
    path: INFRA_PATH,
    query: { tab: "middleware" },
  });
  assert.deepEqual(buildInfraRoute({ tab: "hosts" }), {
    path: INFRA_PATH,
    query: { tab: "hosts" },
  });
});
