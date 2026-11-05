import assert from "node:assert/strict";
import { describe, it } from "node:test";
import type { MetricSeries } from "@/types/api/monitoring";
import {
  INFRA_STRIP_ITEMS,
  buildInfraStripReading,
  evaluateCpuStatus,
  evaluateFreshnessStatus,
  evaluateInfraStripStatus,
  formatInfraStripValue,
  worstSeriesLabel,
} from "../infra-health";

function series(values: number[]): MetricSeries {
  return {
    metric: "m",
    label: "m",
    unit: "",
    series: [
      {
        labels: {},
        points: values.map((value, i) => ({ timestamp: i, value })),
      },
    ],
  };
}

describe("infra-health", () => {
  it("status evaluators", () => {
    assert.equal(evaluateFreshnessStatus(10), "green");
    assert.equal(evaluateFreshnessStatus(120), "yellow");
    assert.equal(evaluateFreshnessStatus(400), "red");
    assert.equal(evaluateCpuStatus(50), "green");
    assert.equal(evaluateCpuStatus(80), "yellow");
    assert.equal(evaluateCpuStatus(95), "red");
  });

  it("buildInfraStripReading uses defs", () => {
    assert.deepEqual(INFRA_STRIP_ITEMS.map((i) => i.id), ["pipeline", "hostCpu"]);
    const def = INFRA_STRIP_ITEMS.find((i) => i.id === "hostCpu");
    assert.ok(def);
    const reading = buildInfraStripReading(def, series([95, 10]));
    assert.equal(reading.status, "green");
    assert.equal(reading.value, 10);
    assert.equal(evaluateInfraStripStatus("pipeline", 200), "yellow");
  });

  it("formatInfraStripValue", () => {
    assert.equal(formatInfraStripValue("pipeline", 12.4), "12s");
    assert.equal(formatInfraStripValue("hostCpu", 66.66), "66.7%");
    assert.equal(formatInfraStripValue("pipeline", null), "—");
  });

  it("worstSeriesLabel names the stalest table", () => {
    const s: MetricSeries = {
      metric: "pipeline_freshness_seconds",
      label: "x",
      unit: "s",
      series: [
        { labels: { store: "gt", table: "metrics" }, points: [{ timestamp: 1, value: 5 }] },
        { labels: { __name__: "m", store: "ch", table: "request_logs" }, points: [{ timestamp: 1, value: 2898 }] },
      ],
    };
    assert.equal(worstSeriesLabel(s), "ch/request_logs");
    assert.equal(worstSeriesLabel(null), "");
  });
});
