import assert from "node:assert/strict";
import { describe, it } from "node:test";
import type { MetricSeries } from "@/types/api/monitoring";
import {
  INFRA_STRIP_ITEMS,
  buildInfraStripReading,
  evaluateCpuStatus,
  evaluateFreshnessStatus,
  evaluateInfraStripStatus,
  evaluateUpStatus,
  formatInfraStripValue,
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
    assert.equal(evaluateUpStatus(1), "green");
    assert.equal(evaluateUpStatus(0), "red");
    assert.equal(evaluateUpStatus(null), "unknown");
    assert.equal(evaluateFreshnessStatus(10), "green");
    assert.equal(evaluateFreshnessStatus(120), "yellow");
    assert.equal(evaluateFreshnessStatus(400), "red");
    assert.equal(evaluateCpuStatus(50), "green");
    assert.equal(evaluateCpuStatus(80), "yellow");
    assert.equal(evaluateCpuStatus(95), "red");
  });

  it("buildInfraStripReading uses defs", () => {
    const def = INFRA_STRIP_ITEMS.find((i) => i.id === "pg");
    assert.ok(def);
    const reading = buildInfraStripReading(def, series([0, 1]));
    assert.equal(reading.status, "green");
    assert.equal(reading.value, 1);
    assert.equal(evaluateInfraStripStatus("pipeline", 200), "yellow");
  });

  it("formatInfraStripValue", () => {
    assert.equal(formatInfraStripValue("pg", 1), "UP");
    assert.equal(formatInfraStripValue("redis", 0), "DOWN");
    assert.equal(formatInfraStripValue("pipeline", 12.4), "12s");
    assert.equal(formatInfraStripValue("hostCpu", 66.66), "66.7%");
    assert.equal(formatInfraStripValue("pg", null), "—");
  });
});
