import assert from "node:assert/strict";
import { test } from "node:test";
import {
  countGreptimeDedupedOccurrences,
  getInfraCharts,
  GREPTIME_DEDUPED_METRICS,
  infraChartMetrics,
} from "../infra-charts";

test("Greptime write/query charts appear exactly once (pipeline only)", () => {
  const counts = countGreptimeDedupedOccurrences();
  for (const metric of GREPTIME_DEDUPED_METRICS) {
    assert.equal(counts[metric], 1, metric);
  }
  const pipeline = new Set(infraChartMetrics("pipeline"));
  const middleware = new Set(infraChartMetrics("middleware"));
  const hosts = new Set(infraChartMetrics("hosts"));
  for (const metric of GREPTIME_DEDUPED_METRICS) {
    assert.ok(pipeline.has(metric), `pipeline has ${metric}`);
    assert.ok(!middleware.has(metric), `middleware must not have ${metric}`);
    assert.ok(!hosts.has(metric), `hosts must not have ${metric}`);
  }
});

test("middleware keeps greptime memory but not write/query", () => {
  const metrics = infraChartMetrics("middleware");
  assert.ok(metrics.includes("greptime_memory_usage_mb"));
  assert.ok(!metrics.includes("greptime_write_rows"));
  assert.ok(!metrics.includes("greptime_promql_query_p99_ms"));
});

test("hosts container metrics are marked remapContainers", () => {
  const hosts = getInfraCharts("hosts");
  const container = hosts.filter((c) => c.metric.startsWith("container_"));
  assert.ok(container.length >= 1);
  for (const chart of container) {
    assert.equal(chart.remapContainers, true, chart.metric);
  }
});
