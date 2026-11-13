import assert from "node:assert/strict";
import { test } from "node:test";
import {
  hoursToSeconds,
  percentToRatio,
  ratioToPercent,
  secondsToHours,
} from "../reconcilePolicyHelpers";

test("secondsToHours and hoursToSeconds convert cleanly between seconds and hours", () => {
  assert.equal(secondsToHours(86400), 24);
  assert.equal(secondsToHours(1800), 0.5);
  assert.equal(hoursToSeconds(24), 86400);
  assert.equal(hoursToSeconds(0.5), 1800);
});

test("ratioToPercent and percentToRatio keep 5.00% <-> 0.05 parity", () => {
  assert.equal(ratioToPercent(0.05), 5);
  assert.equal(ratioToPercent(0.0125), 1.25);
  assert.equal(percentToRatio(5), 0.05);
  assert.equal(percentToRatio(1.25), 0.0125);
});
