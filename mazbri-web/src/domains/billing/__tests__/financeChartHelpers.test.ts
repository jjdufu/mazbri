import assert from "node:assert/strict";
import { test } from "node:test";
import {
  buildUniformCurrencyYAxisFormatter,
  resolveCurrencyAxisScale,
} from "../helpers/financeChartHelpers";

test("resolveCurrencyAxisScale selects uniform k scale at >= 10000", () => {
  assert.equal(resolveCurrencyAxisScale(0), "raw");
  assert.equal(resolveCurrencyAxisScale(9999.99), "raw");
  assert.equal(resolveCurrencyAxisScale(10000), "k");
  assert.equal(resolveCurrencyAxisScale(18500), "k");
});

test("buildUniformCurrencyYAxisFormatter keeps consistent k unit across entire axis (A-02)", () => {
  const fmt = buildUniformCurrencyYAxisFormatter([1500, 9000, 12000, 18000]);
  assert.equal(fmt(0), "¥0");
  assert.equal(fmt(3000), "¥3k");
  assert.equal(fmt(6000), "¥6k");
  assert.equal(fmt(9000), "¥9k");
  assert.equal(fmt(12000), "¥12k");
  assert.equal(fmt(15000), "¥15k");
  assert.equal(fmt(18000), "¥18k");
});

test("buildUniformCurrencyYAxisFormatter keeps consistent raw currency format below 10k", () => {
  const fmt = buildUniformCurrencyYAxisFormatter([0, 2500, 8000]);
  assert.equal(fmt(0), "¥0");
  assert.equal(fmt(2000), "¥2,000");
  assert.equal(fmt(8000), "¥8,000");
  assert.equal(fmt(-1500), "-¥1,500");
});
