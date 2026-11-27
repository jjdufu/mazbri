import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  centsToYuanString,
  formatCodeFilePath,
  formatDateTime,
  formatDisplayDateYmd,
  formatMoney,
  parseDisplayWallClockMs,
  yuanToCentsExact,
} from "../format";
import { formatAxisTime } from "../ops/time-range";

describe("format utilities (Asia/Shanghai wall-clock SSOT)", () => {
  it("parses East-8 wall-clock strings independent of host timezone", () => {
    const ms = parseDisplayWallClockMs("2026-11-26 08:30:00");
    assert.equal(ms, Date.parse("2026-11-26T00:30:00.000Z"));
    assert.equal(formatDateTime(ms), "2026-11-26 08:30:00");
  });

  it("formats East-8 YYYY-MM-DD without UTC midnight day rollback", () => {
    // 2026-11-25T17:30:00Z is 2026-11-26 01:30:00 in Asia/Shanghai
    const ms = Date.parse("2026-11-25T17:30:00.000Z");
    assert.equal(formatDisplayDateYmd(ms), "2026-11-26");
  });

  it("formats axis time in Asia/Shanghai for both short and multi-day spans", () => {
    const ms = Date.parse("2026-11-26T00:30:00.000Z"); // 08:30 UTC+8
    assert.equal(formatAxisTime(ms, 3600), "08:30");
    assert.equal(formatAxisTime(ms, 86400 * 7), "11-26 08:30");
  });

  it("strips CI runner absolute path prefix from code.file.path", () => {
    assert.equal(
      formatCodeFilePath(
        "/home/runner/work/mazbri/mazbri/mazbri-bin/mazbri-gateway/src/proxy.rs",
      ),
      "mazbri-bin/mazbri-gateway/src/proxy.rs",
    );
    assert.equal(
      formatCodeFilePath("/workspace/rust-proj/mazbri/mazbri-libs/mazbri-core/src/lib.rs"),
      "mazbri-libs/mazbri-core/src/lib.rs",
    );
    assert.equal(formatCodeFilePath("src/main.rs"), "src/main.rs");
  });

  it("converts yuan to exact integer cents without IEEE-754 drift", () => {
    assert.equal(yuanToCentsExact("100"), 10000);
    assert.equal(yuanToCentsExact("19.99"), 1999);
    assert.equal(yuanToCentsExact(19.99), 1999);
    assert.equal(yuanToCentsExact("0.01"), 1);
    assert.equal(yuanToCentsExact("1.999"), null);
    assert.equal(yuanToCentsExact("-5"), null);
    assert.equal(centsToYuanString(1999), "19.99");
    assert.equal(centsToYuanString(1), "0.01");
  });

  it("formats DECIMAL(18,6) money strings with 2..6 adaptive decimals", () => {
    assert.equal(formatMoney("5257"), "5,257.00");
    assert.equal(formatMoney("2750.85"), "2,750.85");
    assert.equal(formatMoney("0.700000"), "0.70");
    assert.equal(formatMoney("0.000015"), "0.000015");
    assert.equal(formatMoney("0.002500"), "0.0025");
    assert.equal(formatMoney(null), "—");
  });
});
