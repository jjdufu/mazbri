/**
 * 后端指标目录的单位（英文缩写）→ 界面单位（i18n）。
 * 业务速率统一为「次/秒」；未收录的单位原样显示。
 */
const UNIT_KEYS: Record<string, string> = {
  "req/s": "ops.units.perSec",
  "ops/s": "ops.units.perSec",
  tps: "ops.units.perSec",
  count: "ops.units.times",
  s: "ops.units.seconds",
  days: "ops.units.days",
  ms: "ops.units.ms",
  "%": "ops.units.percent",
};

export function unitKey(unit: string | null | undefined): string | null {
  if (!unit) return null;
  return UNIT_KEYS[unit] ?? null;
}

export function localizeUnit(
  unit: string | null | undefined,
  t: (key: string) => string,
): string {
  const key = unitKey(unit);
  return key ? t(key) : (unit ?? "");
}
