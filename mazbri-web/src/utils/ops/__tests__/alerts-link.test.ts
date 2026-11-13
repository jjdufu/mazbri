import assert from "node:assert/strict";
import { test } from "node:test";
import {
  ALERTS_PATH,
  buildAlertsRoute,
  computeAlertDrillWindow,
  resolveAlertDrillRoute,
  stripAlertEventIdQuery,
} from "../alerts-link";
import { TRAFFIC_PATH } from "../calls-link";
import { RUNTIME_PATH } from "../runtime-link";

test("buildAlertsRoute defaults to bare path", () => {
  assert.deepEqual(buildAlertsRoute(), { path: ALERTS_PATH, query: {} });
});

test("buildAlertsRoute with eventId forces tab=events", () => {
  assert.deepEqual(buildAlertsRoute({ eventId: "e1" }), {
    path: ALERTS_PATH,
    query: { tab: "events", event_id: "e1" },
  });
});

test("buildAlertsRoute keeps explicit tab", () => {
  assert.deepEqual(buildAlertsRoute({ tab: "rules" }), {
    path: ALERTS_PATH,
    query: { tab: "rules" },
  });
});

test("stripAlertEventIdQuery preserves tab", () => {
  assert.deepEqual(
    stripAlertEventIdQuery({ tab: "events", event_id: "e1", foo: "1" }),
    { tab: "events", foo: "1" },
  );
});

test("resolveAlertDrillRoute maps billing_dead_letters and mazbri_pipeline_freshness_seconds to /ops/runtime?tab=pipeline", () => {
  const admin = { isAdmin: true, isBiz: true, isViewer: false };
  const finance = { isAdmin: false, isBiz: true, isViewer: false };
  const viewer = { isAdmin: false, isBiz: false, isViewer: true };

  assert.deepEqual(
    resolveAlertDrillRoute(
      { labels: { category: "billing" }, expr: "billing_dead_letters" },
      admin,
    ),
    { path: RUNTIME_PATH, query: { tab: "pipeline" } },
  );
  assert.deepEqual(
    resolveAlertDrillRoute(
      {
        labels: { category: "pipeline" },
        expr: "max by (store, table) (mazbri_pipeline_freshness_seconds)",
      },
      finance,
    ),
    { path: RUNTIME_PATH, query: { tab: "pipeline" } },
  );
  assert.equal(
    resolveAlertDrillRoute(
      { labels: { category: "billing" }, expr: "billing_dead_letters" },
      viewer,
    ),
    null,
  );
});

test("resolveAlertDrillRoute maps business alerts to /ops/traffic with widened time window", () => {
  const admin = { isAdmin: true, isBiz: true, isViewer: false };
  const win = computeAlertDrillWindow(
    "2026-11-10T12:00:00Z",
    "2026-11-10T12:05:00Z",
    120,
  );
  assert.ok(win);
  assert.equal(win.start, Math.floor(Date.parse("2026-11-10T12:00:00Z") / 1000) - 1020);

  const route = resolveAlertDrillRoute(
    {
      labels: { category: "business", tenant_code: "T001" },
      startedAt: "2026-11-10T12:00:00Z",
      resolvedAt: "2026-11-10T12:05:00Z",
      forSeconds: 120,
    },
    admin,
  );
  assert.deepEqual(route, {
    path: TRAFFIC_PATH,
    query: {
      tab: "customer",
      tr_preset: "custom",
      tr_start: String(win.start),
      tr_end: String(win.end),
      tenant_code: "T001",
    },
  });
});
