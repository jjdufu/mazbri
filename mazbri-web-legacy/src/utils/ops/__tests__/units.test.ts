import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { localizeUnit, unitKey } from "../units";

describe("units", () => {
  it("maps business rates to one key", () => {
    assert.equal(unitKey("req/s"), "ops.units.perSec");
    assert.equal(unitKey("ops/s"), "ops.units.perSec");
    assert.equal(unitKey(""), null);
    assert.equal(unitKey("B/s"), null);
  });
  it("localizeUnit falls back to raw", () => {
    const t = (k: string) => `T(${k})`;
    assert.equal(localizeUnit("req/s", t), "T(ops.units.perSec)");
    assert.equal(localizeUnit("MB", t), "MB");
    assert.equal(localizeUnit(undefined, t), "");
  });
});
