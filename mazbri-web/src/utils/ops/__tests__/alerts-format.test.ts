import assert from "node:assert/strict";
import { test } from "node:test";
import {
  alertSeverityTag,
  alertStatusTag,
  alertTimelineType,
  formatAlertLabels,
  formatAlertMatchers,
  formatAlertTime,
  isValidAlertEmail,
  parseAlertLabelsText,
  parseAlertListText,
  resolveAlertI18nLabel,
} from "../alerts-format";

test("severity / status / timeline tag mapping", () => {
  assert.equal(alertSeverityTag("critical"), "danger");
  assert.equal(alertSeverityTag("warning"), "warning");
  assert.equal(alertSeverityTag("info"), "info");
  assert.equal(alertStatusTag("firing"), "danger");
  assert.equal(alertStatusTag("resolved"), "success");
  assert.equal(alertTimelineType("resolved", "critical"), "success");
  assert.equal(alertTimelineType("firing", "critical"), "danger");
  assert.equal(alertTimelineType("firing", "warning"), "warning");
});

test("labels / matchers / list parsing", () => {
  assert.equal(formatAlertLabels({ a: "1", b: "2" }), "a=1, b=2");
  assert.equal(formatAlertLabels({}), "—");
  assert.equal(formatAlertMatchers({}), "全部");
  assert.deepEqual(parseAlertLabelsText("a=1\nb=2"), { a: "1", b: "2" });
  assert.equal(parseAlertLabelsText("bad"), null);
  assert.deepEqual(parseAlertListText("a, b;c\nd"), ["a", "b", "c", "d"]);
  assert.equal(isValidAlertEmail("a@b.co"), true);
  assert.equal(isValidAlertEmail("not-an-email"), false);
});

test("formatAlertTime uses Asia/Shanghai wall clock, never raw ms", () => {
  const out = formatAlertTime("2026-11-03T10:45:00.000Z");
  assert.match(out, /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/);
  assert.equal(out, "2026-11-03 18:45:00");
  const fromMs = formatAlertTime(1_762_146_300_000);
  assert.match(fromMs, /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/);
  assert.notEqual(String(fromMs), "1762146300000");
  assert.equal(formatAlertTime(null), "—");
  assert.equal(formatAlertTime(""), "—");
});

test("resolveAlertI18nLabel falls back for unknown status", () => {
  const label = resolveAlertI18nLabel(
    "ops.alerts.status",
    "weird_state",
    (k) => k,
    () => false,
  );
  assert.equal(label, "weird_state");
  const known = resolveAlertI18nLabel(
    "ops.alerts.status",
    "firing",
    (k) => (k === "ops.alerts.status.firing" ? "触发中" : k),
    (k) => k === "ops.alerts.status.firing",
  );
  assert.equal(known, "触发中");
});
