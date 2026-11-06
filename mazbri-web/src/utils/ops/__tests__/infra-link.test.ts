import assert from "node:assert/strict";
import { test } from "node:test";
import { INFRA_PATH, buildInfraRoute } from "../infra-link";
import { RUNTIME_PATH } from "../runtime-link";

test("INFRA_PATH aliases RUNTIME_PATH", () => {
  assert.equal(INFRA_PATH, RUNTIME_PATH);
});

test("buildInfraRoute defaults to bare runtime path", () => {
  assert.deepEqual(buildInfraRoute(), { path: RUNTIME_PATH, query: {} });
});

test("buildInfraRoute with tabs", () => {
  assert.deepEqual(buildInfraRoute({ tab: "pipeline" }), {
    path: RUNTIME_PATH,
    query: { tab: "pipeline" },
  });
  assert.deepEqual(buildInfraRoute({ tab: "middleware" }), {
    path: RUNTIME_PATH,
    query: { tab: "middleware" },
  });
  assert.deepEqual(buildInfraRoute({ tab: "hosts" }), {
    path: RUNTIME_PATH,
    query: { tab: "hosts" },
  });
});
