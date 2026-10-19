import assert from "node:assert/strict";
import { hoppAcPanelRect } from "./geometry.ts";

const panel = hoppAcPanelRect({
  top: 120.4,
  left: 40.6,
  width: 80.2,
  height: 32,
  bottom: 152.4,
});

assert.equal(panel.top, Math.round(152.4) - 1);
assert.equal(panel.left, Math.floor(40.6) - 1);
assert.equal(panel.width, Math.ceil(40.6 + 80.2) - panel.left);
assert.ok(panel.width < 256, "no 256px floor");
assert.ok(panel.top > 120, "must hang below the key cell, never flip up");

const derived = hoppAcPanelRect({ top: 10, left: 0, width: 12, height: 20 });
assert.equal(derived.top, Math.round(30) - 1);
assert.equal(derived.left, -1);
assert.equal(derived.width, 13);

const live = hoppAcPanelRect({
  top: 257,
  left: 294,
  width: 339.328125,
  height: 39,
  bottom: 296,
  right: 633.328125,
});
assert.equal(live.left, 293);
assert.equal(live.width, 341);
assert.ok(
  live.left + live.width >= 633.328125,
  "right edge must cover the 键|值 divider",
);

const integer = hoppAcPanelRect({
  top: 0,
  left: 50,
  width: 100,
  height: 32,
  right: 150,
});
assert.equal(integer.left, 49);
assert.equal(integer.width, 101);

console.log("hopp-autocomplete geometry ok");
