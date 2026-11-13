import assert from "node:assert/strict";
import { describe, test } from "node:test";
import {
  EMPTY_PLACEHOLDER,
  formatBytes,
  formatDate,
  formatDateTime,
  formatDays,
  formatEmpty,
  formatIntervalHuman,
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

  test("formatBytes handles zero, sub-byte fractions, and large units", () => {
    assert.equal(formatBytes(0), "0 B");
    assert.equal(formatBytes(0.25), "0.25 B");
    assert.equal(formatBytes(1024), "1 KB");
    assert.equal(formatBytes(12_000_000), "11.44 MB");
  });

  test("formatIntervalHuman renders human-readable intervals in zh and en", () => {
    assert.equal(formatIntervalHuman(0, "zh"), "0 秒");
    assert.equal(formatIntervalHuman(0, "en"), "0s");
    assert.equal(formatIntervalHuman(30, "zh"), "30 秒");
    assert.equal(formatIntervalHuman(30, "en"), "30s");
    assert.equal(formatIntervalHuman(60, "zh"), "1 分钟");
    assert.equal(formatIntervalHuman(60, "en"), "1m");
    assert.equal(formatIntervalHuman(10800, "zh"), "3 小时");
    assert.equal(formatIntervalHuman(10800, "en"), "3h");
    assert.equal(formatIntervalHuman(50000, "zh"), "13.9 小时");
    assert.equal(formatIntervalHuman(86400, "zh"), "1 天");
    assert.equal(formatIntervalHuman(86400, "en"), "1d");
  });
});
