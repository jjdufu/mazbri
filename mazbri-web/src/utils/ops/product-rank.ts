/** Dashboard / 大屏共用的产品排行行形状（OLAP `product_rank`）。 */
export interface ProductRankRow {
  product_id: string;
  req_count: number;
  error_rate: number;
  p95: number;
  cost: number;
  prev_req_count: number;
}

export interface ZeroCallProductRow {
  product_id: string;
  name?: string;
}

export type ProductRankMetric = "req_count" | "error_rate" | "p95" | "cost";

/** 从 OLAP 响应抽出排行行；缺字段时给安全默认值。 */
export function normalizeProductRankRows(
  rows: readonly Partial<ProductRankRow>[] | null | undefined,
): ProductRankRow[] {
  if (!rows?.length) return [];
  return rows.map((row) => ({
    product_id: String(row.product_id ?? ""),
    req_count: Number(row.req_count ?? 0),
    error_rate: Number(row.error_rate ?? 0),
    p95: Number(row.p95 ?? 0),
    cost: Number(row.cost ?? 0),
    prev_req_count: Number(row.prev_req_count ?? 0),
  }));
}

/** 活跃 product_id 列表（供 zero-calls 差集）。 */
export function activeProductIdsFromRank(
  rows: readonly Pick<ProductRankRow, "product_id">[],
): string[] {
  const ids: string[] = [];
  const seen = new Set<string>();
  for (const row of rows) {
    const id = row.product_id.trim();
    if (!id || seen.has(id)) continue;
    seen.add(id);
    ids.push(id);
  }
  return ids;
}

export function normalizeZeroCallProducts(
  rows: readonly Partial<ZeroCallProductRow>[] | null | undefined,
): ZeroCallProductRow[] {
  if (!rows?.length) return [];
  return rows.map((row) => ({
    product_id: String(row.product_id ?? ""),
    name: row.name != null ? String(row.name) : undefined,
  }));
}
