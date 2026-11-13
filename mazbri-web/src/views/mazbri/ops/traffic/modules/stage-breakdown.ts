import type {
  CallStageAccessRow,
  CallStageEntry,
} from "@/types/api/call-monitor";

export interface StageBar {
  key: string;
  label: string;
  ms: number;
  /** Visual bar width percentage (0..100, minimum 2% when ms > 0). */
  percent: number;
  /** Share of total gateway latency rounded to 1 decimal place (0..100). */
  share: number;
  statusCode: number;
  errorCode: string;
  error: boolean;
}

export interface StageSummaryBar {
  ms: number;
  statusCode: number;
  errorCode: string;
  error: boolean;
}

export interface StageBreakdownModel {
  summaryBar: StageSummaryBar | null;
  subBars: StageBar[];
  totalMs: number;
  unsampledSubStages: boolean;
}

export interface ComputeStageBreakdownOptions {
  calls?: CallStageAccessRow[] | null;
  totalDurationMs?: number | null;
}

const PLATFORM_SUMMARY_STAGE = "platform_summary";

function findGatewayCallDurationMs(
  calls: CallStageAccessRow[] | null | undefined,
): number | undefined {
  if (!calls?.length) return undefined;
  const gatewayRow = calls.find(
    (c) => c.target === "gateway" || c.service === "mazbri-gateway",
  );
  if (gatewayRow && gatewayRow.duration_ms > 0) {
    return gatewayRow.duration_ms;
  }
  const maxCallMs = calls.reduce(
    (acc, c) => Math.max(acc, Number.isFinite(c.duration_ms) ? c.duration_ms : 0),
    0,
  );
  return maxCallMs > 0 ? maxCallMs : undefined;
}

/**
 * Splits `platform_summary` out of sub-stage bars and computes each sub-stage's
 * latency share against total gateway request duration (never summing summary + sub-stages).
 */
export function computeStageBreakdown(
  rows: CallStageEntry[],
  options?: ComputeStageBreakdownOptions,
): StageBreakdownModel {
  const summaryRows = rows.filter(
    (row) => row.stage_name === PLATFORM_SUMMARY_STAGE,
  );
  const subRows = rows.filter(
    (row) => row.stage_name !== PLATFORM_SUMMARY_STAGE,
  );

  const summaryPrimary =
    summaryRows.length > 0
      ? summaryRows.reduce((best, cur) =>
          cur.stage_ms > best.stage_ms ? cur : best,
        )
      : null;

  const summaryMs =
    summaryPrimary && summaryPrimary.stage_ms > 0
      ? summaryPrimary.stage_ms
      : undefined;
  const gatewayCallMs = findGatewayCallDurationMs(options?.calls);
  const propTotalMs =
    options?.totalDurationMs && options.totalDurationMs > 0
      ? options.totalDurationMs
      : undefined;
  const maxSubMs = subRows.reduce(
    (acc, row) => Math.max(acc, Math.max(0, row.stage_ms)),
    0,
  );

  const totalMs = summaryMs ?? gatewayCallMs ?? propTotalMs ?? Math.max(maxSubMs, 1);

  const summaryBar: StageSummaryBar | null = summaryPrimary
    ? {
        ms: Math.max(0, summaryPrimary.stage_ms),
        statusCode: summaryPrimary.status_code ?? 0,
        errorCode: summaryPrimary.error_code ?? "",
        error:
          Boolean(summaryPrimary.error_code) ||
          (summaryPrimary.status_code ?? 0) >= 400,
      }
    : null;

  const subBars: StageBar[] = subRows.map((row, index) => {
    const ms = Math.max(0, row.stage_ms);
    const rawPct = totalMs > 0 ? (ms / totalMs) * 100 : 0;
    const clampedPct = Math.min(100, Math.max(0, rawPct));
    const share = Number(clampedPct.toFixed(1));
    const percent = clampedPct > 0 ? Math.max(2, share) : 0;
    return {
      key: `${row.request_id}-${row.attempt_order}-${row.stage_name}-${index}`,
      label:
        row.attempt_order > 0
          ? `${row.service}/${row.stage_name} #${row.attempt_order}`
          : `${row.service}/${row.stage_name}`,
      ms,
      percent,
      share,
      statusCode: row.status_code ?? 0,
      errorCode: row.error_code ?? "",
      error: Boolean(row.error_code) || (row.status_code ?? 0) >= 400,
    };
  });

  return {
    summaryBar,
    subBars,
    totalMs,
    unsampledSubStages: summaryBar !== null && subBars.length === 0,
  };
}

export function formatStageShare(share: number): string {
  return Number.isInteger(share) ? `${share}%` : `${share.toFixed(1)}%`;
}
