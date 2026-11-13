import assert from "node:assert/strict";
import { describe, test } from "node:test";
import {
  EMPTY_PLACEHOLDER,
  formatDate,
  formatDateTime,
  formatDays,
  formatEmpty,
  formatMoney,
} from "../format";

describe("format display helpers (step12)", () => {
  test("formatMoney uses two decimals and thousand separators", () => {
    assert.equal(formatMoney(61917.05), "61,917.05");
    assert.equal(formatMoney("100029"), "100,029.00");
    assert.equal(formatMoney("1611.2142857142857"), "1,611.21");
    assert.equal(formatMoney(null), "—");
    assert.equal(formatMoney(""), "—");
  });

  test("formatDays collapses long float tails to one decimal", () => {
    assert.equal(formatDays(38.42881145542404), "38.4");
    assert.equal(formatDays(10), "10");
    assert.equal(formatDays(10.04), "10");
    assert.equal(formatDays(null), "—");
  });

  test("formatDateTime renders Asia/Shanghai wall clock, never raw ms", () => {
    // 2024-11-03 00:00:00Z → 2024-11-03 08:00:00 +08
    const iso = "2024-11-03T00:00:00.000Z";
    assert.equal(formatDateTime(iso), "2024-11-03 08:00:00");
    assert.equal(formatDate(iso), "2024-11-03");
    const ms = Date.parse(iso);
    assert.equal(formatDateTime(ms), "2024-11-03 08:00:00");
    assert.notEqual(formatDateTime(ms), String(ms));
  });

  test("formatDateTime accepts unix seconds", () => {
    // 2024-11-03T00:00:00Z
    assert.equal(formatDateTime(1730592000), "2024-11-03 08:00:00");
  });

  test("formatEmpty standardizes empty values to em dash", () => {
    assert.equal(EMPTY_PLACEHOLDER, "—");
    assert.equal(formatEmpty(null), "—");
    assert.equal(formatEmpty(undefined), "—");
    assert.equal(formatEmpty(""), "—");
    assert.equal(formatEmpty(0), "0");
    assert.equal(formatEmpty("ok"), "ok");
  });
});
