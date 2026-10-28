import assert from "node:assert/strict";
import { test } from "node:test";
import {
  classifySubmittedCode,
  formatExpiryUtc8,
  formatMmSs,
  isWarning,
  parseBroadcast,
  toClientDeadline,
} from "./mfa-step-up-state";

test("formatMmSs 按分钟累加，负数当 0", () => {
  assert.equal(formatMmSs(0), "00:00");
  assert.equal(formatMmSs(59), "00:59");
  assert.equal(formatMmSs(60), "01:00");
  assert.equal(formatMmSs(61), "01:01");
  assert.equal(formatMmSs(3599), "59:59");
  assert.equal(formatMmSs(3600), "60:00");
  assert.equal(formatMmSs(3661), "61:01");
  assert.equal(formatMmSs(-5), "00:00");
  assert.equal(formatMmSs(Number.NaN), "00:00");
});

test("isWarning 边界：大于 0 且不超过 120 秒", () => {
  assert.equal(isWarning(0), false);
  assert.equal(isWarning(-1), false);
  assert.equal(isWarning(1), true);
  assert.equal(isWarning(120), true);
  assert.equal(isWarning(121), false);
  assert.equal(isWarning(Number.NaN), false);
});

test("toClientDeadline 只使用 remaining_secs，下限按 0", () => {
  const now = 1_700_000_000_000;
  assert.equal(toClientDeadline(0, now), now);
  assert.equal(toClientDeadline(-30, now), now);
  assert.equal(toClientDeadline(Number.NaN, now), now);
  assert.equal(toClientDeadline(15, now), now + 15_000);
  assert.equal(toClientDeadline(1.5, now), now + 1_500);
});

test("formatExpiryUtc8 使用 Asia/Shanghai", () => {
  assert.equal(
    formatExpiryUtc8("2026-10-27T07:15:00Z"),
    "2026-10-27 15:15:00 (UTC+8)",
  );
  assert.equal(formatExpiryUtc8("not-a-date"), "");
});

test("parseBroadcast 只接受 {type:changed}", () => {
  assert.deepEqual(parseBroadcast({ type: "changed" }), { type: "changed" });
  assert.equal(parseBroadcast({ type: "changed", extra: 1 }), null);
  assert.equal(parseBroadcast({ type: "Changed" }), null);
  assert.equal(parseBroadcast({ type: "refresh" }), null);
  assert.equal(parseBroadcast("{\"type\":\"changed\"}"), null);
  assert.equal(parseBroadcast(null), null);
  assert.equal(parseBroadcast(undefined), null);
  assert.equal(parseBroadcast(["changed"]), null);
  assert.equal(parseBroadcast(1), null);
});

test("classifySubmittedCode 有且只有一个码", () => {
  assert.deepEqual(classifySubmittedCode("123456", 6), { totp_code: "123456" });
  assert.deepEqual(classifySubmittedCode("12345678", 8), {
    totp_code: "12345678",
  });
  assert.deepEqual(classifySubmittedCode("abcd2345", 6), {
    backup_code: "ABCD2345",
  });
  assert.equal(classifySubmittedCode("123456", 8), null);
  const backup = classifySubmittedCode("ABCD2345", 6);
  assert.deepEqual(backup, { backup_code: "ABCD2345" });
  assert.equal(backup !== null && "totp_code" in backup, false);
  assert.equal(classifySubmittedCode("IO000000", 6), null);
  assert.equal(classifySubmittedCode("", 6), null);
});
