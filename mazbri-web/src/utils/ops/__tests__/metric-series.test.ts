import assert from "node:assert/strict";
import { describe, it } from "node:test";
import type { MetricSeries } from "@/types/api/monitoring";
import {
  aggregateLastMetricValue,
  lastPointValue,
  mapLastPointValues,
} from "../metric-series";

function series(pointsLists: number[][]): MetricSeries {
  return {
    metric: "m",
    label: "m",
    unit: "",
    series: pointsLists.map((pts, i) => ({
      labels: { i: String(i) },
      points: pts.map((value, j) => ({ timestamp: j, value })),
    })),
  };
}

describe("metric-series helpers", () => {
  it("lastPointValue", () => {
    assert.equal(lastPointValue(undefined), null);
    assert.equal(lastPointValue([]), null);
    assert.equal(lastPointValue([{ timestamp: 1, value: 3.5 }]), 3.5);
    assert.equal(
      lastPointValue([
        { timestamp: 1, value: 1 },
        { timestamp: 2, value: Number.NaN },
      ]),
      null,
    );
  });

  it("mapLastPointValues and aggregates", () => {
    assert.deepEqual(mapLastPointValues(null), []);
    const s = series([
      [1, 2],
      [10, 20],
    ]);
    assert.deepEqual(mapLastPointValues(s), [2, 20]);
    assert.equal(aggregateLastMetricValue(s, "min"), 2);
    assert.equal(aggregateLastMetricValue(s, "max"), 20);
    assert.equal(aggregateLastMetricValue(s, "sum"), 22);
    assert.equal(aggregateLastMetricValue(s, "avg"), 11);
    assert.equal(aggregateLastMetricValue(s, "first"), 2);
    assert.equal(aggregateLastMetricValue(null, "max"), null);
  });
});
