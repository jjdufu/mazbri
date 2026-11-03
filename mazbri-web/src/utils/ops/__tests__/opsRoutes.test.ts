import assert from "node:assert/strict";
import { test } from "node:test";
import {
  ALERTS_PATH,
  BILLING_PATH,
  DASHBOARD_PATH,
  FLOW_PATH,
  INFRA_PATH,
  OPS_SECURITY_PATH,
  RELIABILITY_PATH,
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
  buildServicesRoute,
  buildTrafficRoute,
  buildUpstreamRoute,
} from "../opsRoutes";

test("opsRoutes re-exports merged IA parent paths", () => {
  assert.equal(DASHBOARD_PATH, "/dashboard");
  assert.equal(SCREEN_PATH, "/screen");
  assert.equal(INFRA_PATH, "/ops/infra");
  assert.equal(ALERTS_PATH, "/ops/alerts");
  assert.equal(BILLING_PATH, "/ops/billing");
  assert.equal(TRAFFIC_PATH, "/ops/traffic");
  assert.equal(UPSTREAM_PATH, "/ops/upstream");
  assert.equal(FLOW_PATH, "/ops/flow");
  assert.equal(SERVICES_PATH, "/ops/services");
  assert.equal(RELIABILITY_PATH, "/ops/reliability");
  assert.equal(TRACES_PATH, "/ops/traces");
  assert.equal(OPS_SECURITY_PATH, "/ops/security");
});

test("opsRoutes builders write ?tab= for deep links", () => {
  assert.deepEqual(buildInfraRoute({ tab: "hosts" }), {
    path: INFRA_PATH,
    query: { tab: "hosts" },
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
  assert.deepEqual(buildServicesRoute({ tab: "topology" }), {
    path: SERVICES_PATH,
    query: { tab: "topology" },
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
