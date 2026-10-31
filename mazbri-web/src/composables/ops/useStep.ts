/** Allowed PromQL/timeseries step sizes (seconds). */
export const STEP_CANDIDATES = [15, 60, 300, 3600] as const;

/**
 * Adaptive step: rangeSeconds / 300, rounded up to the next candidate
 * in {15s, 1m, 5m, 1h}. Mirrors analytics adaptive window rules.
 */
export function resolveStep(rangeSeconds: number): number {
  if (!Number.isFinite(rangeSeconds) || rangeSeconds <= 0) {
    return STEP_CANDIDATES[0];
  }
  const raw = rangeSeconds / 300;
  for (const candidate of STEP_CANDIDATES) {
    if (raw <= candidate) {
      return candidate;
    }
  }
  return STEP_CANDIDATES[STEP_CANDIDATES.length - 1];
}

export function useStep() {
  return { resolveStep, STEP_CANDIDATES };
}
