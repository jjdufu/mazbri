import assert from "node:assert/strict";
import { test } from "node:test";
import { RELIABILITY_PATH, buildReliabilityRoute } from "../reliability-link";

test("buildReliabilityRoute returns bare path", () => {
  assert.deepEqual(buildReliabilityRoute(), { path: RELIABILITY_PATH });
});

test("RELIABILITY_PATH is merged IA leaf", () => {
  assert.equal(RELIABILITY_PATH, "/ops/reliability");
});
