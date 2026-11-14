import assert from "node:assert/strict";
import { test } from "node:test";
import {
  TRACE_RETENTION_MS,
  buildTraceRoute,
  buildTraceSearchRoute,
  canDrillToTrace,
  isTraceExpired,
  unixSecondsOf,
} from "../trace-link";

test("unixSecondsOf handles ISO, ms and ns", () => {
  assert.equal(unixSecondsOf("2026-11-01T03:38:00.250Z"), 1793504280);
  assert.equal(unixSecondsOf(1793504280250), 1793504280);
  assert.equal(unixSecondsOf(1793504280250000000), 1793504280);
  assert.equal(unixSecondsOf(""), undefined);
  assert.equal(unixSecondsOf("nope"), undefined);
});

test("buildTraceRoute carries start and end in seconds", () => {
  assert.deepEqual(
    buildTraceRoute("abc", { start: 1793504280, end: 1793507880 }),
    {
      path: "/ops/traces/abc",
      query: { start: "1793504280", end: "1793507880" },
    },
  );
});

test("buildTraceSearchRoute points to /ops/traffic?tab=traces", () => {
  assert.deepEqual(
    buildTraceSearchRoute({
      filters: {
        service: "mazbri-gateway",
        errors_only: true,
        include_internal: true,
      },
    }),
    {
      path: "/ops/traffic",
      query: {
        tab: "traces",
        service: "mazbri-gateway",
        errors_only: "true",
        include_internal: "true",
      },
    },
  );
});

test("canDrillToTrace blocks non-admin and >7d expired traces", () => {
  const nowMs = 1_800_000_000_000;
  const freshMs = nowMs - 3600_000;
  const expiredMs = nowMs - TRACE_RETENTION_MS - 1_000;

  assert.equal(isTraceExpired(freshMs, nowMs), false);
  assert.equal(isTraceExpired(expiredMs, nowMs), true);

  assert.equal(canDrillToTrace(freshMs, true, nowMs), true);
  assert.equal(canDrillToTrace(freshMs, false, nowMs), false);
  assert.equal(canDrillToTrace(expiredMs, true, nowMs), false);
});
