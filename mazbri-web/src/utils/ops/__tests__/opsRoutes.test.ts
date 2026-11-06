import assert from "node:assert/strict";
import { test } from "node:test";
import {
  ALERTS_PATH,
  BILLING_PATH,
  DASHBOARD_PATH,
  FLOW_PATH,
  INFRA_PATH,
  OPS_SECURITY_PATH,
  RUNTIME_PATH,
  SCREEN_PATH,
  SERVICES_PATH,
  TRACES_PATH,
  TRAFFIC_PATH,
  UPSTREAM_PATH,
  buildAlertsRoute,
  buildBillingRoute,
  buildFlowRoute,
  buildInfraRoute,
  buildDashboardHref,
  buildScreenHref,
  buildRuntimeRoute,
  buildServicesRoute,
  buildTrafficRoute,
  buildUpstreamRoute,
} from "../opsRoutes";

test("opsRoutes re-exports merged IA parent paths", () => {
  assert.equal(DASHBOARD_PATH, "/dashboard");
  assert.equal(SCREEN_PATH, "/screen");
  assert.equal(RUNTIME_PATH, "/ops/runtime");
  assert.equal(INFRA_PATH, RUNTIME_PATH);
  assert.equal(SERVICES_PATH, RUNTIME_PATH);
  assert.equal(ALERTS_PATH, "/ops/alerts");
  assert.equal(BILLING_PATH, "/ops/billing");
  assert.equal(TRAFFIC_PATH, "/ops/traffic");
  assert.equal(UPSTREAM_PATH, "/ops/upstream");
  assert.equal(FLOW_PATH, "/ops/flow");
  assert.equal(TRACES_PATH, "/ops/traces");
  assert.equal(OPS_SECURITY_PATH, "/ops/security");
});

test("opsRoutes builders write ?tab= for deep links", () => {
  assert.deepEqual(buildInfraRoute({ tab: "hosts" }), {
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
  assert.deepEqual(buildTrafficRoute({ tab: "latency" }), {
    path: TRAFFIC_PATH,
    query: { tab: "latency" },
  });
  assert.deepEqual(buildUpstreamRoute({ tab: "breaker" }), {
    path: UPSTREAM_PATH,
    query: { tab: "breaker" },
  });
  assert.deepEqual(buildFlowRoute({ tab: "cache" }), {
    path: FLOW_PATH,
    query: { tab: "cache" },
  });
  assert.deepEqual(buildServicesRoute({ tab: "health" }), {
    path: RUNTIME_PATH,
    query: { tab: "apps" },
  });
});

test("buildScreenHref uses hash SCREEN_PATH", () => {
  assert.equal(
    buildScreenHref("http://192.168.50.50:8085", "/"),
    "http://192.168.50.50:8085/#/screen",
  );
});

test("buildDashboardHref uses hash DASHBOARD_PATH", () => {
  assert.equal(
    buildDashboardHref("http://192.168.50.50:8085", "/"),
    "http://192.168.50.50:8085/#/dashboard",
  );
});
