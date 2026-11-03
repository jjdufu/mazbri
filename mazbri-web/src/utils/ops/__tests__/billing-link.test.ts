import assert from "node:assert/strict";
import { test } from "node:test";
import { BILLING_PATH, buildBillingRoute } from "../billing-link";

test("buildBillingRoute defaults to bare path", () => {
  assert.deepEqual(buildBillingRoute(), { path: BILLING_PATH, query: {} });
});

test("buildBillingRoute with health tab", () => {
  assert.deepEqual(buildBillingRoute({ tab: "health" }), {
    path: BILLING_PATH,
    query: { tab: "health" },
  });
});

test("buildBillingRoute with reconcile tab", () => {
  assert.deepEqual(buildBillingRoute({ tab: "reconcile" }), {
    path: BILLING_PATH,
    query: { tab: "reconcile" },
  });
});
