import assert from "node:assert/strict";
import { test } from "node:test";
import {
  alertSeverityTag,
  alertStatusTag,
  alertTimelineType,
  formatAlertLabels,
  formatAlertMatchers,
  isValidAlertEmail,
  parseAlertLabelsText,
  parseAlertListText,
} from "../alerts-format";

test("severity / status / timeline tag mapping", () => {
  assert.equal(alertSeverityTag("critical"), "danger");
  assert.equal(alertSeverityTag("warning"), "warning");
  assert.equal(alertSeverityTag("info"), "info");
  assert.equal(alertStatusTag("firing"), "danger");
  assert.equal(alertStatusTag("acked"), "warning");
  assert.equal(alertStatusTag("resolved"), "success");
  assert.equal(alertTimelineType("resolved", "critical"), "success");
  assert.equal(alertTimelineType("firing", "critical"), "danger");
  assert.equal(alertTimelineType("firing", "warning"), "warning");
});

test("labels / matchers / list parsing", () => {
  assert.equal(formatAlertLabels({ a: "1", b: "2" }), "a=1, b=2");
  assert.equal(formatAlertLabels({}), "—");
  assert.equal(formatAlertMatchers({}), "(all)");
  assert.deepEqual(parseAlertLabelsText("a=1\nb=2"), { a: "1", b: "2" });
  assert.equal(parseAlertLabelsText("bad"), null);
  assert.deepEqual(parseAlertListText("a, b;c\nd"), ["a", "b", "c", "d"]);
  assert.equal(isValidAlertEmail("a@b.co"), true);
  assert.equal(isValidAlertEmail("not-an-email"), false);
});
