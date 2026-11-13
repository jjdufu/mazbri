import assert from "node:assert/strict";
import { test } from "node:test";
import {
  canOverviewDrill,
  resolveOverviewDrill,
  type OverviewDrillRole,
} from "../overview-drill";
import { CALLS_PATH } from "../calls-link";
import { RUNTIME_PATH } from "../runtime-link";

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

test("ADMIN can drill circuit / ratelimit / billing pipeline / latency", () => {
  assert.deepEqual(resolveOverviewDrill("3.2", ADMIN), {
    path: RUNTIME_PATH,
    query: { tab: "apps" },
  });
  assert.deepEqual(resolveOverviewDrill("4.1", ADMIN), {
    path: CALLS_PATH,
  });
  assert.deepEqual(resolveOverviewDrill("6.1", ADMIN), {
    path: RUNTIME_PATH,
    query: { tab: "pipeline" },
  });
  assert.deepEqual(resolveOverviewDrill("2.2", ADMIN), {
    path: CALLS_PATH,
  });
});

test("FINANCE cannot open breaker but can open ratelimit/billing pipeline", () => {
  assert.equal(resolveOverviewDrill("3.2", FINANCE), null);
  assert.equal(canOverviewDrill("3.2", FINANCE), false);
  assert.deepEqual(resolveOverviewDrill("4.1", FINANCE), {
    path: CALLS_PATH,
  });
  assert.deepEqual(resolveOverviewDrill("6.1", FINANCE), {
    path: RUNTIME_PATH,
    query: { tab: "pipeline" },
  });
});

test("VIEWER remaps latency/errors to calls and blocks breaker/ratelimit/billing", () => {
  assert.deepEqual(resolveOverviewDrill("2.2", VIEWER), {
    path: CALLS_PATH,
  });
  assert.deepEqual(resolveOverviewDrill("2.4", VIEWER), {
    path: CALLS_PATH,
  });
  assert.equal(resolveOverviewDrill("3.2", VIEWER), null);
  assert.equal(resolveOverviewDrill("4.1", VIEWER), null);
  assert.equal(resolveOverviewDrill("6.1", VIEWER), null);
  assert.deepEqual(resolveOverviewDrill("2.1", VIEWER), {
    path: CALLS_PATH,
  });
});
