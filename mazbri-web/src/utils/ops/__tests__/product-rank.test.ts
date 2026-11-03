import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  activeProductIdsFromRank,
  normalizeProductRankRows,
  normalizeZeroCallProducts,
} from "../product-rank";

describe("product-rank helpers", () => {
  it("normalizeProductRankRows fills defaults", () => {
    const rows = normalizeProductRankRows([
      { product_id: "p1", req_count: 10 },
      {},
    ]);
    assert.equal(rows.length, 2);
    assert.deepEqual(rows[0], {
      product_id: "p1",
      req_count: 10,
      error_rate: 0,
      p95: 0,
      cost: 0,
      prev_req_count: 0,
    });
    assert.equal(rows[1]?.product_id, "");
  });

  it("normalizeProductRankRows handles empty", () => {
    assert.deepEqual(normalizeProductRankRows(undefined), []);
    assert.deepEqual(normalizeProductRankRows(null), []);
    assert.deepEqual(normalizeProductRankRows([]), []);
  });

  it("activeProductIdsFromRank dedupes and drops blanks", () => {
    assert.deepEqual(
      activeProductIdsFromRank([
        { product_id: "a" },
        { product_id: " a " },
        { product_id: "" },
        { product_id: "b" },
        { product_id: "a" },
      ]),
      ["a", "b"],
    );
  });

  it("normalizeZeroCallProducts", () => {
    assert.deepEqual(normalizeZeroCallProducts([{ product_id: "z", name: "Z" }]), [
      { product_id: "z", name: "Z" },
    ]);
    assert.deepEqual(normalizeZeroCallProducts(undefined), []);
  });
});
