import assert from "node:assert/strict";
import { test } from "node:test";
import { BILLING_PATH, buildBillingRoute } from "../billing-link";

test("BILLING_PATH points to consolidated /operations/bills", () => {
  assert.equal(BILLING_PATH, "/operations/bills");
  assert.deepEqual(buildBillingRoute(), { path: BILLING_PATH, query: {} });
});

test("buildBillingRoute with overview / bills / reconcile tabs", () => {
  assert.deepEqual(buildBillingRoute({ tab: "overview" }), {
    path: "/operations/bills",
    query: { tab: "overview" },
  });
  assert.deepEqual(buildBillingRoute({ tab: "bills" }), {
    path: "/operations/bills",
    query: { tab: "bills" },
  });
  assert.deepEqual(buildBillingRoute({ tab: "reconcile" }), {
    path: "/operations/bills",
    query: { tab: "reconcile" },
  });
});
