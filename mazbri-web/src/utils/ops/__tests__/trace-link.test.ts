import assert from "node:assert/strict";
import { test } from "node:test";
import { buildTraceRoute, unixSecondsOf } from "../trace-link";

test("unixSecondsOf handles ISO, ms and ns", () => {
  assert.equal(unixSecondsOf("2026-11-01T03:38:00.250Z"), 1793504280);
  assert.equal(unixSecondsOf(1793504280250), 1793504280);
  assert.equal(unixSecondsOf(1793504280250000000), 1793504280);
  assert.equal(unixSecondsOf(""), undefined);
  assert.equal(unixSecondsOf("nope"), undefined);
});

test("buildTraceRoute carries start in seconds", () => {
  assert.deepEqual(buildTraceRoute("abc", { start: 1793504280 }), {
    path: "/ops/traces/abc",
    query: { start: "1793504280" },
  });
});
