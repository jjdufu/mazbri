import assert from "node:assert/strict";
import { test } from "node:test";
import {
  defaultTimeRange,
  formatAxisTime,
  formatClockHm,
  resolveRange,
  pickerRangeOf,
  PRESET_SECONDS,
  rangeFromQuery,
  sameRange,
} from "../time-range";

test("rangeFromQuery reads Unix seconds from the route query", () => {
  const r = rangeFromQuery({
    tr_preset: "custom",
    tr_start: "1793504100",
    tr_end: "1793505000",
  });
  assert.deepEqual(r, { preset: "custom", start: 1793504100, end: 1793505000 });
});

test("rangeFromQuery rejects NaN, inverted or unknown presets", () => {
  assert.equal(rangeFromQuery({ tr_preset: "custom", tr_start: "1", tr_end: "NaN" }), null);
  assert.equal(rangeFromQuery({ tr_preset: "custom", tr_start: "5", tr_end: "4" }), null);
  assert.equal(rangeFromQuery({ tr_preset: "2h", tr_start: "1", tr_end: "4" }), null);
  assert.equal(rangeFromQuery({}), null);
});

test("pickerRangeOf yields epoch-ms numbers that land in 2026", () => {
  const [a, b] = pickerRangeOf({ start: 1793504100, end: 1793505000 });
  assert.equal(typeof a, "number");
  assert.equal(new Date(a).getUTCFullYear(), 2026);
  assert.equal(new Date(b).toISOString(), "2026-11-01T03:50:00.000Z");
});

test("sameRange compares preset and bounds", () => {
  const v = { preset: "custom" as const, start: 1, end: 2 };
  assert.ok(sameRange(v, { ...v }));
  assert.ok(!sameRange(v, { ...v, end: 3 }));
  assert.ok(!sameRange(null, v));
});

test("defaultTimeRange uses preset seconds", () => {
  const before = Math.floor(Date.now() / 1000);
  const range = defaultTimeRange("1h");
  const after = Math.floor(Date.now() / 1000);
  assert.equal(range.preset, "1h");
  assert.ok(range.end >= before && range.end <= after);
  assert.equal(range.end - range.start, PRESET_SECONDS["1h"]);
});

test("formatClockHm renders Asia/Shanghai wall clock", () => {
  // 2024-01-01T00:05:00Z → 08:05 (UTC+8)
  assert.equal(formatClockHm(1704067500), "08:05");
});

test("formatAxisTime adds the date only for windows over 24h", () => {
  assert.equal(formatAxisTime(1704067500_000, 3600), "08:05");
  assert.equal(formatAxisTime(1704067500_000, 7 * 86400), "01-01 08:05");
});

test("resolveRange rolls presets to now and keeps custom ranges", () => {
  const rolled = resolveRange({ preset: "6h", start: 1, end: 2 }, 100_000);
  assert.deepEqual(rolled, { preset: "6h", start: 100_000 - 6 * 3600, end: 100_000 });
  const custom = { preset: "custom" as const, start: 10, end: 500 };
  assert.deepEqual(resolveRange(custom, 100_000), custom);
});

