import assert from "node:assert/strict";
import { test } from "node:test";
import type { CallStageEntry } from "@/types/api/call-monitor";
import { TRACE_RETENTION_MS, canDrillToTrace } from "@/utils/ops/trace-link";
import { resolveWaterfallDurPlacement } from "@/views/mazbri/ops/traces/waterfall-layout";
import {
  computeStageBreakdown,
  formatStageShare,
} from "../modules/stage-breakdown";

function makeStage(
  stage_name: string,
  stage_ms: number,
  overrides: Partial<CallStageEntry> = {},
): CallStageEntry {
  return {
    timestamp: "2026-11-14T00:00:00Z",
    service: "mazbri-gateway",
    target: "gateway",
    request_id: "req-1",
    attempt_order: 0,
    stage_name,
    stage_ms,
    status_code: 200,
    error_code: "",
    error_stage: "",
    upstream_mode: "direct",
    result: "ok",
    ...overrides,
  };
}

test("computeStageBreakdown separates platform_summary and computes sub-stage share against totalMs", () => {
  const model = computeStageBreakdown([
    makeStage("platform_summary", 103),
    makeStage("upstream_http", 101, { attempt_order: 1 }),
  ]);

  assert.ok(model.summaryBar);
  assert.equal(model.summaryBar.ms, 103);
  assert.equal(model.unsampledSubStages, false);
  assert.equal(model.subBars.length, 1);
  assert.equal(model.subBars[0]?.ms, 101);
  assert.equal(model.subBars[0]?.share, 98.1);
  assert.equal(formatStageShare(model.subBars[0]!.share), "98.1%");
});

test("computeStageBreakdown flags unsampledSubStages when only platform_summary exists", () => {
  const model = computeStageBreakdown([makeStage("platform_summary", 42)]);
  assert.ok(model.summaryBar);
  assert.equal(model.summaryBar.ms, 42);
  assert.equal(model.subBars.length, 0);
  assert.equal(model.unsampledSubStages, true);
});

test("computeStageBreakdown falls back to gateway call duration when platform_summary is absent", () => {
  const model = computeStageBreakdown(
    [makeStage("auth_verify", 10), makeStage("upstream_http", 80)],
    {
      calls: [
        {
          timestamp: "2026-11-14T00:00:00Z",
          service: "mazbri-gateway",
          target: "gateway",
          request_id: "req-1",
          trace_id: "0123456789abcdef0123456789abcdef",
          tenant_code: "T001",
          supplier_code: "S001",
          channel_code: "C001",
          attempt_order: 0,
          path: "/v1/query",
          status_code: 200,
          duration_ms: 100,
          upstream_service_time_ms: 80,
          internal_ms: 20,
          error_code: "",
        },
      ],
    },
  );
  assert.equal(model.summaryBar, null);
  assert.equal(model.totalMs, 100);
  assert.equal(model.subBars[0]?.share, 10);
  assert.equal(model.subBars[1]?.share, 80);
});

test("7-day trace retention guard blocks expired rows and non-admin roles", () => {
  const now = 1_800_000_000_000;
  assert.equal(canDrillToTrace(now - 6 * 86400_000, true, now), true);
  assert.equal(
    canDrillToTrace(now - TRACE_RETENTION_MS - 60_000, true, now),
    false,
  );
  assert.equal(canDrillToTrace(now - 60_000, false, now), false);
});

test("resolveWaterfallDurPlacement places duration labels across all 3 zones without right overflow", () => {
  // Zone 1: early/mid bar -> outside-right
  const early = resolveWaterfallDurPlacement(10, 25);
  assert.equal(early.mode, "outside-right");
  assert.equal(early.style.left, "calc(35% + 6px)");

  // Zone 2: wide bar reaching right zone -> inside-bar
  const root = resolveWaterfallDurPlacement(0, 100);
  assert.equal(root.mode, "inside-bar");
  assert.equal(root.style.right, "calc(0% + 6px)");

  const longChild = resolveWaterfallDurPlacement(20, 75);
  assert.equal(longChild.mode, "inside-bar");
  assert.equal(longChild.style.right, "calc(5% + 6px)");

  // Zone 3: short tail bar near 100% -> outside-left
  const tail = resolveWaterfallDurPlacement(92, 8);
  assert.equal(tail.mode, "outside-left");
  assert.equal(tail.style.right, "calc(8% + 6px)");
});
