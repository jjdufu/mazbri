import assert from "node:assert/strict";
import { test } from "node:test";
import {
  ALERTS_PATH,
  buildAlertsRoute,
  stripAlertEventIdQuery,
} from "../alerts-link";

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
