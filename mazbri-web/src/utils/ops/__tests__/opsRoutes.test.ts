import assert from "node:assert/strict";
import { test } from "node:test";
import {
  ALERTS_PATH,
  BILLING_PATH,
  DASHBOARD_PATH,
  INFRA_PATH,
  NODES_PATH,
  PIPELINE_PATH,
  TRACE_DETAIL_BASE_PATH,
  TRAFFIC_PATH,
  buildAlertsRoute,
  buildBillingRoute,
  buildCallsRoute,
  buildDashboardHref,
  buildInfraRoute,
  buildNodesRoute,
  buildPipelineRoute,
  buildTraceSearchRoute,
} from "../opsRoutes";

test("opsRoutes re-exports split IA parent paths", () => {
  assert.equal(DASHBOARD_PATH, "/dashboard");
  assert.equal(NODES_PATH, "/ops/nodes");
  assert.equal(INFRA_PATH, "/ops/infra");
  assert.equal(PIPELINE_PATH, "/ops/pipeline");
  assert.equal(ALERTS_PATH, "/ops/alerts");
  assert.equal(BILLING_PATH, "/operations/bills");
  assert.equal(TRAFFIC_PATH, "/ops/traffic");
  assert.equal(TRACE_DETAIL_BASE_PATH, "/ops/traces");
});

test("opsRoutes builders write ?tab= and ?service= for deep links", () => {
  assert.deepEqual(buildNodesRoute({ service: "gateway" }), {
    path: NODES_PATH,
    query: { service: "gateway" },
  });
  assert.deepEqual(buildInfraRoute({ tab: "hosts" }), {
    path: INFRA_PATH,
    query: { tab: "hosts" },
  });
  assert.deepEqual(buildPipelineRoute({ tab: "storage" }), {
    path: PIPELINE_PATH,
    query: { tab: "storage" },
  });
  assert.deepEqual(buildAlertsRoute({ tab: "rules" }), {
    path: ALERTS_PATH,
    query: { tab: "rules" },
  });
  assert.deepEqual(buildBillingRoute({ tab: "reconcile" }), {
    path: BILLING_PATH,
    query: { tab: "reconcile" },
  });
  assert.deepEqual(buildCallsRoute({ tab: "upstream" }), {
    path: TRAFFIC_PATH,
    query: { tab: "upstream" },
  });
  assert.deepEqual(buildTraceSearchRoute(), {
    path: TRAFFIC_PATH,
    query: { tab: "traces" },
  });
});

test("buildDashboardHref uses hash DASHBOARD_PATH", () => {
  assert.equal(
    buildDashboardHref("https://example.com", "/app"),
    "https://example.com/app#/dashboard",
  );
});
