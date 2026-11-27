export type CurrencyAxisScale = "k" | "raw";

/**
 * 依据整张图的最大绝对值决定唯一 Y 轴量纲（核销 A-02）：
 * - 当最大值 >= 10,000 时，整条 Y 轴统一使用 `¥Xk`（如 `¥0`, `¥3k`, `¥6k`, `¥9k`, `¥12k`），
 *   绝不出现下半 `9,000.00`、上半突切 `12k` 的断裂；
 * - 否则整条 Y 轴统一使用带千分位的 `¥X`（如 `¥0`, `¥1,000`, `¥2,000`）。
 */
export function resolveCurrencyAxisScale(
  maxAbsValue: number,
): CurrencyAxisScale {
  if (!Number.isFinite(maxAbsValue)) return "raw";
  return Math.abs(maxAbsValue) >= 10_000 ? "k" : "raw";
}

export function formatCurrencyAxisTick(
  value: number,
  scale: CurrencyAxisScale,
  symbol = "¥",
): string {
  if (!Number.isFinite(value)) return `${symbol}0`;
  if (value === 0) return `${symbol}0`;
  const sign = value < 0 ? "-" : "";
  const abs = Math.abs(value);
  if (scale === "k") {
    const scaled = parseFloat((abs / 1000).toFixed(1));
    return `${sign}${symbol}${scaled}k`;
  }
  const rounded = parseFloat(abs.toFixed(2));
  return `${sign}${symbol}${rounded.toLocaleString("en-US", {
    maximumFractionDigits: 2,
  })}`;
}

export function buildUniformCurrencyYAxisFormatter(
  values: readonly number[],
  symbol = "¥",
): (value: number) => string {
  let maxAbs = 0;
  for (const v of values) {
    if (Number.isFinite(v)) {
      const abs = Math.abs(v);
      if (abs > maxAbs) maxAbs = abs;
    }
  }
  const scale = resolveCurrencyAxisScale(maxAbs);
  return (val: number) => formatCurrencyAxisTick(val, scale, symbol);
}
