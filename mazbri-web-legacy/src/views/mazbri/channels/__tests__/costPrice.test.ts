import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  COST_PRICE_PATTERN,
  formatCostPrice,
  isValidCostPrice,
  normalizeCostPriceInput,
} from "../costPrice";

describe("channel costPrice decimal string helpers", () => {
  it("formats NUMERIC(18,6) strings without float loss or trailing noise", () => {
    assert.equal(formatCostPrice("0.700000"), "0.70");
    assert.equal(formatCostPrice("0.002500"), "0.0025");
    assert.equal(formatCostPrice("0.000050"), "0.00005");
    assert.equal(formatCostPrice("0.000001"), "0.000001");
    assert.equal(formatCostPrice("0"), "0.00");
    assert.equal(formatCostPrice("0.000000"), "0.00");
    assert.equal(formatCostPrice(""), "0.00");
    assert.equal(formatCostPrice(undefined), "0.00");
    assert.equal(formatCostPrice("0012.3000"), "12.30");
    assert.equal(
      formatCostPrice("999999999999.123456"),
      "999999999999.123456",
    );
  });

  it("validates 1-12 integer digits and up to 6 fractional digits", () => {
    assert.ok(COST_PRICE_PATTERN.test("0"));
    assert.ok(isValidCostPrice("0.000050"));
    assert.ok(isValidCostPrice("123456789012.654321"));
    assert.equal(isValidCostPrice("-0.01"), false);
    assert.equal(isValidCostPrice("0.0000001"), false);
    assert.equal(isValidCostPrice("1234567890123.00"), false);
    assert.equal(isValidCostPrice("1."), false);
    assert.equal(isValidCostPrice(".5"), false);
    assert.equal(isValidCostPrice("abc"), false);
  });

  it("normalizes valid form inputs on blur and leaves invalid inputs untouched", () => {
    assert.equal(normalizeCostPriceInput(" 0.000050 "), "0.00005");
    assert.equal(normalizeCostPriceInput("0.7"), "0.70");
    assert.equal(normalizeCostPriceInput(""), "0.00");
    assert.equal(normalizeCostPriceInput("0.1234567"), "0.1234567");
  });
});
