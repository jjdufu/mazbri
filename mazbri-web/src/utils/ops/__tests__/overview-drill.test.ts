import assert from "node:assert/strict";
import { test } from "node:test";
import {
  canOverviewDrill,
  resolveOverviewDrill,
  type OverviewDrillRole,
} from "../overview-drill";
import { BILLING_PATH } from "../billing-link";
import { FLOW_PATH } from "../flow-link";
import { TRAFFIC_PATH } from "../traffic-link";
import { UPSTREAM_PATH } from "../upstream-link";

const ADMIN: OverviewDrillRole = {
  isAdmin: true,
  isBiz: true,
  isViewer: false,
};
const FINANCE: OverviewDrillRole = {
  isAdmin: false,
  isBiz: true,
  isViewer: false,
};
const VIEWER: OverviewDrillRole = {
  isAdmin: false,
  isBiz: false,
  isViewer: true,
};

test("ADMIN can drill circuit / ratelimit / billing / latency", () => {
  assert.deepEqual(resolveOverviewDrill("3.2", ADMIN), {
    path: UPSTREAM_PATH,
    query: { tab: "breaker" },
  });
  assert.deepEqual(resolveOverviewDrill("4.1", ADMIN), {
    path: FLOW_PATH,
    query: { tab: "ratelimit" },
  });
  assert.deepEqual(resolveOverviewDrill("6.1", ADMIN), {
    path: BILLING_PATH,
    query: { tab: "health" },
  });
  assert.deepEqual(resolveOverviewDrill("2.2", ADMIN), {
    path: TRAFFIC_PATH,
    query: { tab: "latency" },
  });
});

test("FINANCE cannot open breaker but can open ratelimit/billing", () => {
  assert.equal(resolveOverviewDrill("3.2", FINANCE), null);
  assert.equal(canOverviewDrill("3.2", FINANCE), false);
  assert.deepEqual(resolveOverviewDrill("4.1", FINANCE), {
    path: FLOW_PATH,
    query: { tab: "ratelimit" },
  });
  assert.deepEqual(resolveOverviewDrill("6.1", FINANCE), {
    path: BILLING_PATH,
    query: { tab: "health" },
  });
});

test("VIEWER remaps latency/errors to RED and blocks breaker/ratelimit/billing", () => {
  assert.deepEqual(resolveOverviewDrill("2.2", VIEWER), {
    path: TRAFFIC_PATH,
    query: { tab: "red" },
  });
  assert.deepEqual(resolveOverviewDrill("2.4", VIEWER), {
    path: TRAFFIC_PATH,
    query: { tab: "red" },
  });
  assert.equal(resolveOverviewDrill("3.2", VIEWER), null);
  assert.equal(resolveOverviewDrill("4.1", VIEWER), null);
  assert.equal(resolveOverviewDrill("6.1", VIEWER), null);
  assert.deepEqual(resolveOverviewDrill("2.1", VIEWER), {
    path: TRAFFIC_PATH,
    query: { tab: "red" },
  });
});
