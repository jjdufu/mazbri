/**
 * Dashboard / 大屏共用的产品排行（OLAP `product_rank`）。
 *
 * 口径：网关调用日志分钟/小时聚合；error_rate 仅计 5xx（0–1）；p95 为调用耗时 ms；
 * prev_req_count 为紧邻的上一个等长窗口。
 */
export interface ProductRankRow {
  product_id: string;
  req_count: number;
  error_rate: number;
  p95: number;
  cost: number;
  prev_req_count: number;
}

/** 带显示名的排行行。 */
export interface NamedProductRankRow extends ProductRankRow {
  name: string;
}

export interface ProductCatalogEntry {
  product_id: string;
  name: string | null;
}

export interface ZeroCallProductRow {
  product_id: string;
  name: string;
}

export type ProductRankMetric = "req_count" | "error_rate" | "p95" | "cost";

/** 从 OLAP 响应抽出排行行；缺字段时给安全默认值。 */
export function normalizeProductRankRows(
  rows: readonly Partial<ProductRankRow>[] | null | undefined,
): ProductRankRow[] {
  if (!rows?.length) return [];
  return rows.map((row) => ({
    product_id: String(row.product_id ?? "").trim(),
    req_count: Number(row.req_count ?? 0),
    error_rate: Number(row.error_rate ?? 0),
    p95: Number(row.p95 ?? 0),
    cost: Number(row.cost ?? 0),
    prev_req_count: Number(row.prev_req_count ?? 0),
  }));
}

/** product_id → 显示名（名录缺名或未收录时回退为 id）。 */
export function productNameMap(
  catalog: readonly ProductCatalogEntry[] | null | undefined,
): Map<string, string> {
  const map = new Map<string, string>();
  for (const entry of catalog ?? []) {
    const id = String(entry.product_id ?? "").trim();
    if (!id) continue;
    const name = entry.name?.trim();
    map.set(id, name || id);
  }
  return map;
}

export function withProductNames(
  rows: readonly ProductRankRow[],
  names: ReadonlyMap<string, string>,
): NamedProductRankRow[] {
  return rows.map((row) => ({
    ...row,
    name: names.get(row.product_id) ?? row.product_id,
  }));
}

/**
 * 环比：当前窗相对上一窗的变化（-0.2 = 下降 20%）。
 * 上一窗为 0 时返回 null（显示「新增」而非 0 / ∞）。
 */
export function productRankDelta(row: Pick<ProductRankRow, "req_count" | "prev_req_count">): number | null {
  if (!(row.prev_req_count > 0)) return null;
  return row.req_count / row.prev_req_count - 1;
}
