import assert from "node:assert/strict";
import { test } from "node:test";
import {
  ALERTS_PATH,
  buildAlertsRoute,
  resolveAlertDrillRoute,
  stripAlertEventIdQuery,
} from "../alerts-link";
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
