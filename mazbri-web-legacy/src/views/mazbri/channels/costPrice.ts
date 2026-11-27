/**
 * 对齐后端 `NUMERIC(18,6)`（`rust_decimal::Decimal`）契约的成本单价字符串处理工具。
 * 全程采用十进制字符串切片与正则校验，禁止经由 `Number` / `parseFloat` 转换，杜绝 IEEE-754 精度漂移。
 */
export const COST_PRICE_PATTERN = /^\d{1,12}(\.\d{1,6})?$/;

export function isValidCostPrice(value: string | null | undefined): boolean {
  if (typeof value !== "string") {
    return false;
  }
  const trimmed = value.trim();
  return COST_PRICE_PATTERN.test(trimmed);
}

/**
 * 列表与详情展示格式化：保留有效小数位并去除冗余尾零，最少保留 2 位小数。
 * 示例：
 * - `"0.700000"` -> `"0.70"`
 * - `"0.002500"` -> `"0.0025"`
 * - `"0.000050"` -> `"0.00005"`
 * - `"0"` / `""` -> `"0.00"`
 */
export function formatCostPrice(raw: string | null | undefined): string {
  const trimmed = (raw ?? "").trim();
  if (!trimmed) {
    return "0.00";
  }
  if (!COST_PRICE_PATTERN.test(trimmed)) {
    return trimmed;
  }
  const dotIndex = trimmed.indexOf(".");
  const rawInt = dotIndex >= 0 ? trimmed.slice(0, dotIndex) : trimmed;
  const rawFrac = dotIndex >= 0 ? trimmed.slice(dotIndex + 1) : "";

  const normalizedInt = rawInt.replace(/^0+(?=\d)/, "") || "0";
  const trimmedFrac = rawFrac.replace(/0+$/, "");
  const normalizedFrac =
    trimmedFrac.length >= 2 ? trimmedFrac : trimmedFrac.padEnd(2, "0");

  return `${normalizedInt}.${normalizedFrac}`;
}

/**
 * 表单失焦规整：合法十进制字符串自动规整整数前导零与多余尾零（最少保留 2 位小数），
 * 非法输入保持原样交由表单规则拦截报错。
 */
export function normalizeCostPriceInput(raw: string | null | undefined): string {
  const trimmed = (raw ?? "").trim();
  if (!trimmed) {
    return "0.00";
  }
  if (!COST_PRICE_PATTERN.test(trimmed)) {
    return trimmed;
  }
  return formatCostPrice(trimmed);
}
