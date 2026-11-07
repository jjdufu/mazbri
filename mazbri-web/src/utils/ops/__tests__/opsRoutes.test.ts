import assert from "node:assert/strict";
import { test } from "node:test";
import {
  ALERTS_PATH,
  BILLING_PATH,
  DASHBOARD_PATH,
  RUNTIME_PATH,
  TRACES_PATH,
  buildAlertsRoute,
  buildBillingRoute,
  buildDashboardHref,
  buildRuntimeRoute,
} from "../opsRoutes";

test("opsRoutes re-exports merged IA parent paths", () => {
  assert.equal(DASHBOARD_PATH, "/dashboard");
  assert.equal(RUNTIME_PATH, "/ops/runtime");
  assert.equal(ALERTS_PATH, "/ops/alerts");
  assert.equal(BILLING_PATH, "/ops/billing");
  assert.equal(TRACES_PATH, "/ops/traces");
});

test("opsRoutes builders write ?tab= for deep links", () => {
  assert.deepEqual(buildRuntimeRoute({ tab: "hosts" }), {
    path: RUNTIME_PATH,
    query: { tab: "hosts" },
  });
  assert.deepEqual(buildRuntimeRoute({ tab: "apps" }), {
    path: RUNTIME_PATH,
    query: { tab: "apps" },
  });
  assert.deepEqual(buildAlertsRoute({ tab: "rules" }), {
    path: ALERTS_PATH,
    query: { tab: "rules" },
  });
  assert.deepEqual(buildBillingRoute({ tab: "reconcile" }), {
    path: BILLING_PATH,
    query: { tab: "reconcile" },
  });
});

test("buildDashboardHref uses hash DASHBOARD_PATH", () => {
  assert.equal(
    buildDashboardHref("http://192.168.50.50:8085", "/"),
    "http://192.168.50.50:8085/#/dashboard",
  );
});
