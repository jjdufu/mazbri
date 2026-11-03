import assert from "node:assert/strict";
import { test } from "node:test";
import { getFlowCacheLoadCharts, getFlowCharts } from "../flow-charts";

test("ratelimit charts include decisions and quota", () => {
  const metrics = getFlowCharts("ratelimit").map((c) => c.metric);
  assert.ok(metrics.includes("ratelimit_decisions"));
  assert.ok(metrics.includes("gateway_quota_usage_ratio"));
  assert.equal(metrics.length, 7);
});

test("cache grid omits entries; load charts include entries", () => {
  const grid = getFlowCharts("cache").map((c) => c.metric);
  assert.ok(grid.includes("cache_hit_ratio"));
  assert.ok(grid.includes("cache_memory_mb"));
  assert.ok(!grid.includes("cache_entries"));
  const load = getFlowCacheLoadCharts().map((c) => c.metric);
  assert.ok(load.includes("cache_entries"));
  assert.equal(load.length, grid.length + 1);
});
