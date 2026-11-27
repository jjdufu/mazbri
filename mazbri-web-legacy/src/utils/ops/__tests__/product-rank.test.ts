import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  normalizeProductRankRows,
  productNameMap,
  productRankDelta,
  withProductNames,
} from "../product-rank";

describe("product-rank helpers", () => {
  it("normalizeProductRankRows fills defaults", () => {
    const rows = normalizeProductRankRows([
      { product_id: " p1 ", req_count: 10 },
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

  it("names fall back to id", () => {
    const names = productNameMap([
      { product_id: "1001", name: "身份核验" },
      { product_id: "1002", name: null },
      { product_id: "", name: "x" },
    ]);
    assert.equal(names.get("1001"), "身份核验");
    assert.equal(names.get("1002"), "1002");
    assert.equal(names.size, 2);
    const rows = withProductNames(
      normalizeProductRankRows([{ product_id: "1001" }, { product_id: "9" }]),
      names,
    );
    assert.equal(rows[0]?.name, "身份核验");
    assert.equal(rows[1]?.name, "9");
  });

  it("productRankDelta", () => {
    assert.equal(productRankDelta({ req_count: 5, prev_req_count: 0 }), null);
    assert.equal(productRankDelta({ req_count: 15, prev_req_count: 10 }), 0.5);
    assert.equal(productRankDelta({ req_count: 0, prev_req_count: 10 }), -1);
  });
});
