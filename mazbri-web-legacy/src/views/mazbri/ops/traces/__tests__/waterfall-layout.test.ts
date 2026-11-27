import assert from "node:assert/strict";
import test from "node:test";
import {
  computeSelfSegments,
  computeTreeConnectors,
  createWaterfallProjector,
  findFoldableUpstream,
  formatTraceMs,
  resolveWaterfallDurPlacement,
} from "../waterfall-layout";

test("resolveWaterfallDurPlacement chooses non-overflowing side", () => {
  assert.equal(resolveWaterfallDurPlacement(10, 25).mode, "outside-right");
  assert.equal(resolveWaterfallDurPlacement(15, 80).mode, "inside-bar");
  assert.equal(resolveWaterfallDurPlacement(92, 5).mode, "outside-left");
});

test("formatTraceMs formats nanoseconds into unified millisecond strings", () => {
  assert.equal(formatTraceMs(0), "0.00 ms");
  assert.equal(formatTraceMs(4_000), "<0.01 ms");
  assert.equal(formatTraceMs(770_000), "0.77 ms");
  assert.equal(formatTraceMs(87_780_000), "87.78 ms");
  assert.equal(formatTraceMs(12_345_600_000), "12345.6 ms");
});

test("computeTreeConnectors draws accurate ancestor pipes and branch elbows", () => {
  // 0: root
  // 1: ├─ request_prepare
  // 2: │  ├─ transport
  // 3: │  │  └─ redis.set_nx_ex
  // 2: │  └─ balance_check
  // 1: └─ upstream
  // 2:    └─ upstream.attempt
  const connectors = computeTreeConnectors([0, 1, 2, 3, 2, 1, 2]);
  assert.deepEqual(connectors, [
    { guides: [], connector: "none" },
    { guides: [], connector: "tee" },
    { guides: ["pipe"], connector: "tee" },
    { guides: ["pipe", "pipe"], connector: "elbow" },
    { guides: ["pipe"], connector: "elbow" },
    { guides: [], connector: "elbow" },
    { guides: ["blank"], connector: "elbow" },
  ]);
});

test("computeSelfSegments isolates uncovered active intervals on container spans", () => {
  const parent = {
    span_id: "attempt",
    parent_span_id: "upstream",
    start_ns: 0,
    end_ns: 100_000_000,
  };
  const children = [
    {
      span_id: "bypass",
      parent_span_id: "attempt",
      start_ns: 10_000_000,
      end_ns: 90_000_000,
    },
  ];
  const segs = computeSelfSegments(parent, children, (ns) => ns / 1_000_000);
  assert.equal(segs.length, 2);
  assert.deepEqual(segs[0], { left: 0, width: 10 });
  assert.deepEqual(segs[1], { left: 90, width: 10 });
});

test("findFoldableUpstream and createWaterfallProjector expand pre/post stages", () => {
  const spans = [
    {
      span_id: "root",
      parent_span_id: null,
      service_name: "mazbri-gateway",
      span_name: "POST /api/v1/proxy",
      start_ns: 0,
      end_ns: 100_000_000,
      child_count: 2,
    },
    {
      span_id: "wire",
      parent_span_id: "bypass",
      service_name: "mazbri-bypass",
      span_name: "upstream_wire",
      span_kind: "SPAN_KIND_CLIENT",
      start_ns: 5_000_000,
      end_ns: 93_000_000,
      child_count: 0,
    },
  ];
  const foldable = findFoldableUpstream(0, 100_000_000, spans);
  assert.deepEqual(foldable, { startNs: 5_000_000, endNs: 93_000_000 });

  const proj = createWaterfallProjector(
    0,
    100_000_000,
    "fold-upstream",
    foldable,
  );
  assert.ok(proj.breakZone !== null);
  assert.equal(proj.breakZone.width, 22);
  // 5 ms pre-stage expands from 5% (linear) to > 30% in folded mode:
  const preWidth = proj.project(5_000_000) - proj.project(0);
  assert.ok(preWidth >= 30, `expected preWidth >= 30, got ${preWidth}`);
  // 7 ms post-stage expands from 7% (linear) to > 35% in folded mode:
  const postWidth = proj.project(100_000_000) - proj.project(93_000_000);
  assert.ok(postWidth >= 35, `expected postWidth >= 35, got ${postWidth}`);
});
