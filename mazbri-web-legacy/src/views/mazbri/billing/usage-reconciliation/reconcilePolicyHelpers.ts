export function secondsToHours(seconds: number): number {
  if (!Number.isFinite(seconds) || seconds <= 0) return 24;
  const hours = seconds / 3600;
  return Number.isInteger(hours) ? hours : Number(hours.toFixed(2));
}

export function hoursToSeconds(hours: number): number {
  if (!Number.isFinite(hours) || hours <= 0) return 3600;
  return Math.max(60, Math.round(hours * 3600));
}

export function ratioToPercent(ratio: number): number {
  if (!Number.isFinite(ratio) || ratio < 0) return 0;
  return Number((ratio * 100).toFixed(2));
}

export function percentToRatio(percent: number): number {
  if (!Number.isFinite(percent) || percent < 0) return 0;
  const clamped = Math.min(100, percent);
  return Number((clamped / 100).toFixed(6));
}
