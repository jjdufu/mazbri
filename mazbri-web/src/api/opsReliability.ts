import request from "@/utils/http";
import { API_BASE_PATH } from "@/config/api";

const BASE = `${API_BASE_PATH}/stats/ops`;

export interface AnomalyEvent {
  id: string;
  detected_at: string;
  kind: string;
  severity: string;
  entity_type: string;
  entity_id: string;
  metric: string;
  value: number;
  baseline: number;
  score: number;
  detail: Record<string, unknown>;
  status: string;
}

/** 时间窗 [start, end)（Unix 秒）内最近的异常事件。 */
export function listAnomalies(params: { start: number; end: number; limit?: number }) {
  return request.get<AnomalyEvent[]>({
    url: `${BASE}/anomalies`,
    params,
  });
}

export interface ProductName {
  product_id: string;
  name: string | null;
}

/** 已启用产品 id → 名称（TopN 显示名）。 */
export function listProductNames() {
  return request.get<ProductName[]>({ url: `${BASE}/products/names` });
}

export interface ZeroCallProductItem {
  product_id: string;
  name: string | null;
}

export interface ZeroCallProductsPage {
  items: ZeroCallProductItem[];
  total: number;
  page: number;
  page_size: number;
}

/** 服务端分页获取窗口 `[start, end)` 内零调用已启用产品。 */
export function listZeroCallProducts(params: {
  start: number;
  end: number;
  page?: number;
  page_size?: number;
  q?: string;
}) {
  return request.get<ZeroCallProductsPage>({
    url: `${BASE}/products/zero-calls`,
    params,
  });
}

export interface SloDefinition {
  id: string;
  name: string;
  sli_type: string;
  query_kind: string;
  query: string;
  objective: number;
  window_days: number;
  scope: string;
  enabled: boolean;
}

export interface SloStatusRow {
  name: string;
  sli?: number;
  error_budget_remaining?: number;
  burn_1h?: number;
  burn_5m?: number;
  burn_page?: number;
  burn_ticket?: number;
  ts?: string;
}

export function listSlos() {
  return request.get<{ items: SloDefinition[] }>({ url: `${BASE}/slos` });
}

export function listSloStatus() {
  return request.get<{ definitions: SloDefinition[]; status: SloStatusRow[] }>({
    url: `${BASE}/slos/status`,
  });
}
