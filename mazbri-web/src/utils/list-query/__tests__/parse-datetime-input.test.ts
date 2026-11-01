import assert from "node:assert/strict";
import { test } from "node:test";
import { parseDatetimeInput } from "../parse-datetime-input";

// 1793504280 = 2026-11-01 03:38:00 UTC = 2026-11-01 11:38:00 Beijing
test("Unix seconds and milliseconds become Beijing wall clock", () => {
  assert.equal(parseDatetimeInput("1793504280"), "2026-11-01 11:38:00.000");
  assert.equal(parseDatetimeInput("1793504280000"), "2026-11-01 11:38:00.000");
  assert.equal(parseDatetimeInput(1793504280), "2026-11-01 11:38:00.000");
});

test("offset-qualified ISO becomes Beijing wall clock", () => {
  assert.equal(
    parseDatetimeInput("2026-11-01T03:38:00Z"),
    "2026-11-01 11:38:00.000",
  );
});

test("naive wall clock is kept as typed", () => {
  assert.equal(
    parseDatetimeInput("2026-11-01 11:38"),
    "2026-11-01 11:38:00.000",
  );
});
