export type WaterfallDurMode = "outside-right" | "inside-bar" | "outside-left";

export interface WaterfallDurPlacement {
  mode: WaterfallDurMode;
  style: {
    left?: string;
    right?: string;
  };
}

export type TreeGuide = "pipe" | "blank";
export type TreeConnector = "none" | "tee" | "elbow";

export interface TreeRowIndent {
  guides: TreeGuide[];
  connector: TreeConnector;
}

export interface WaterfallSegment {
  left: number;
  width: number;
}

export interface SpanIntervalLike {
  span_id: string;
  parent_span_id: string | null;
  service_name?: string;
  span_name?: string;
  span_kind?: string;
  start_ns: number;
  end_ns: number;
  child_count?: number;
}

export type WaterfallScaleMode = "linear" | "fold-upstream";

export interface FoldableInterval {
  startNs: number;
  endNs: number;
}

export interface WaterfallTick {
  percent: number;
  label: string;
}

export interface WaterfallProjector {
  project: (ns: number) => number;
  ticks: WaterfallTick[];
  breakZone: WaterfallSegment | null;
}

const RIGHT_SAFE_LIMIT_PCT = 70;
const INSIDE_MIN_WIDTH_PCT = 30;
const FOLD_UPSTREAM_MIN_SHARE = 0.6;
const FOLD_UPSTREAM_ZONE_PCT = 22;
const FOLD_SIDE_MIN_PCT = 12;

export function resolveWaterfallDurPlacement(
  left: number,
  width: number,
): WaterfallDurPlacement {
  const safeLeft = Math.max(0, Math.min(100, left));
  const safeWidth = Math.max(0, Math.min(100 - safeLeft, width));
  const end = safeLeft + safeWidth;

  if (end <= RIGHT_SAFE_LIMIT_PCT) {
    return {
      mode: "outside-right",
      style: { left: `calc(${end}% + 6px)` },
    };
  }

  if (safeWidth >= INSIDE_MIN_WIDTH_PCT) {
    const rightGap = Math.max(0, 100 - end);
    return {
      mode: "inside-bar",
      style: { right: `calc(${rightGap}% + 6px)` },
    };
  }

  const fromRightToBarStart = Math.max(0, 100 - safeLeft);
  return {
    mode: "outside-left",
    style: { right: `calc(${fromRightToBarStart}% + 6px)` },
  };
}

export function formatTraceMs(ns: number): string {
  if (!Number.isFinite(ns) || ns <= 0) return "0.00 ms";
  const ms = ns / 1e6;
  if (ms < 0.01) return "<0.01 ms";
  if (ms >= 10_000) return `${ms.toFixed(1)} ms`;
  return `${ms.toFixed(2)} ms`;
}

function hasSubsequentSiblingAtLevel(
  depths: readonly number[],
  fromRow: number,
  targetDepth: number,
): boolean {
  for (let j = fromRow + 1; j < depths.length; j += 1) {
    const dj = depths[j] ?? 0;
    if (dj < targetDepth) return false;
    if (dj === targetDepth) return true;
  }
  return false;
}

export function computeTreeConnectors(
  depths: readonly number[],
): TreeRowIndent[] {
  return depths.map((depth, idx) => {
    if (depth <= 0) {
      return { guides: [], connector: "none" };
    }
    const guides: TreeGuide[] = [];
    for (let level = 1; level < depth; level += 1) {
      guides.push(
        hasSubsequentSiblingAtLevel(depths, idx, level) ? "pipe" : "blank",
      );
    }
    const connector: TreeConnector = hasSubsequentSiblingAtLevel(
      depths,
      idx,
      depth,
    )
      ? "tee"
      : "elbow";
    return { guides, connector };
  });
}

function mergeIntervals(raw: Array<[number, number]>): Array<[number, number]> {
  if (!raw.length) return [];
  const sorted = [...raw].sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  const merged: Array<[number, number]> = [sorted[0]!];
  for (let i = 1; i < sorted.length; i += 1) {
    const prev = merged[merged.length - 1]!;
    const cur = sorted[i]!;
    if (cur[0] <= prev[1]) {
      prev[1] = Math.max(prev[1], cur[1]);
    } else {
      merged.push([cur[0], cur[1]]);
    }
  }
  return merged;
}

export function computeSelfSegments(
  parent: SpanIntervalLike,
  directChildren: readonly SpanIntervalLike[],
  project: (ns: number) => number,
): WaterfallSegment[] {
  if (!directChildren.length || parent.end_ns <= parent.start_ns) return [];
  const clamped: Array<[number, number]> = [];
  for (const c of directChildren) {
    const s = Math.max(parent.start_ns, Math.min(parent.end_ns, c.start_ns));
    const e = Math.max(parent.start_ns, Math.min(parent.end_ns, c.end_ns));
    if (e > s) clamped.push([s, e]);
  }
  if (!clamped.length) {
    const left = project(parent.start_ns);
    const right = project(parent.end_ns);
    return right > left ? [{ left, width: right - left }] : [];
  }
  const merged = mergeIntervals(clamped);
  const out: WaterfallSegment[] = [];
  let cursor = parent.start_ns;
  for (const [cs, ce] of merged) {
    if (cs > cursor) {
      const left = project(cursor);
      const right = project(cs);
      if (right - left >= 0.15) {
        out.push({ left, width: Math.max(0.25, right - left) });
      }
    }
    cursor = Math.max(cursor, ce);
  }
  if (parent.end_ns > cursor) {
    const left = project(cursor);
    const right = project(parent.end_ns);
    if (right - left >= 0.15) {
      out.push({ left, width: Math.max(0.25, right - left) });
    }
  }
  return out;
}

function isStorageSpanName(name: string): boolean {
  return (
    name.startsWith("redis.") ||
    name.startsWith("db.") ||
    name.startsWith("cache.")
  );
}

export function findFoldableUpstream(
  traceStartNs: number,
  traceEndNs: number,
  spans: readonly SpanIntervalLike[],
): FoldableInterval | null {
  const totalNs = traceEndNs - traceStartNs;
  if (totalNs <= 0) return null;

  const wireSpans = spans.filter(
    (s) =>
      s.span_name === "upstream_wire" || s.span_name === "upstream_wire#retry",
  );
  const candidates = wireSpans.length
    ? wireSpans
    : spans.filter(
        (s) =>
          s.service_name === "mazbri-bypass" &&
          s.span_kind === "SPAN_KIND_CLIENT" &&
          !isStorageSpanName(s.span_name ?? ""),
      );
  const fallback = candidates.length
    ? candidates
    : spans.filter(
        (s) =>
          s.service_name === "mazbri-bypass" &&
          (s.child_count ?? 0) === 0 &&
          !isStorageSpanName(s.span_name ?? ""),
      );
  if (!fallback.length) return null;

  let best: FoldableInterval | null = null;
  let bestDur = 0;
  for (const s of fallback) {
    const startNs = Math.max(traceStartNs, Math.min(traceEndNs, s.start_ns));
    const endNs = Math.max(startNs, Math.min(traceEndNs, s.end_ns));
    const dur = endNs - startNs;
    if (dur > bestDur) {
      bestDur = dur;
      best = { startNs, endNs };
    }
  }
  if (!best || bestDur / totalNs <= FOLD_UPSTREAM_MIN_SHARE) return null;
  return best;
}

function formatAxisTickMs(ns: number, totalNs: number): string {
  if (ns <= 0) return "0 ms";
  const totalMs = totalNs / 1e6;
  const ms = ns / 1e6;
  if (totalMs < 10) return `${ms.toFixed(2)} ms`;
  if (totalMs < 100) return `${ms.toFixed(1)} ms`;
  return `${ms.toFixed(0)} ms`;
}

export function createWaterfallProjector(
  traceStartNs: number,
  traceEndNs: number,
  mode: WaterfallScaleMode,
  foldable: FoldableInterval | null,
): WaterfallProjector {
  const totalNs = Math.max(1, traceEndNs - traceStartNs);

  if (mode !== "fold-upstream" || !foldable) {
    const project = (ns: number): number => {
      const pct = ((ns - traceStartNs) / totalNs) * 100;
      return Math.max(0, Math.min(100, pct));
    };
    const ticks: WaterfallTick[] = [0, 25, 50, 75, 100].map((percent) => ({
      percent,
      label: formatAxisTickMs((totalNs * percent) / 100, totalNs),
    }));
    return { project, ticks, breakZone: null };
  }

  const preNs = Math.max(0, foldable.startNs - traceStartNs);
  const midNs = Math.max(1, foldable.endNs - foldable.startNs);
  const postNs = Math.max(0, traceEndNs - foldable.endNs);
  const remPct = 100 - FOLD_UPSTREAM_ZONE_PCT;

  let prePct = 0;
  let postPct = 0;
  if (preNs > 0 && postNs > 0) {
    const rawPre = (preNs / (preNs + postNs)) * remPct;
    prePct = Math.max(
      FOLD_SIDE_MIN_PCT,
      Math.min(remPct - FOLD_SIDE_MIN_PCT, rawPre),
    );
    postPct = remPct - prePct;
  } else if (preNs > 0) {
    prePct = remPct;
  } else if (postNs > 0) {
    postPct = remPct;
  }

  const midEndPct = prePct + FOLD_UPSTREAM_ZONE_PCT;
  const project = (ns: number): number => {
    const clamped = Math.max(traceStartNs, Math.min(traceEndNs, ns));
    if (clamped <= foldable.startNs) {
      return preNs > 0 ? ((clamped - traceStartNs) / preNs) * prePct : 0;
    }
    if (clamped <= foldable.endNs) {
      return (
        prePct + ((clamped - foldable.startNs) / midNs) * FOLD_UPSTREAM_ZONE_PCT
      );
    }
    return (
      midEndPct +
      (postNs > 0 ? ((clamped - foldable.endNs) / postNs) * postPct : 0)
    );
  };

  const ticks: WaterfallTick[] = [{ percent: 0, label: "0 ms" }];
  if (prePct >= 10) {
    ticks.push({ percent: prePct, label: formatTraceMs(preNs) });
  }
  if (midEndPct <= 90) {
    ticks.push({ percent: midEndPct, label: formatTraceMs(preNs + midNs) });
  }
  ticks.push({ percent: 100, label: formatTraceMs(totalNs) });

  return {
    project,
    ticks,
    breakZone: { left: prePct, width: FOLD_UPSTREAM_ZONE_PCT },
  };
}
