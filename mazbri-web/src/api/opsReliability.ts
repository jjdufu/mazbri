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



export function listAnomalies(limit = 50, kind?: string) {
  return request.get<AnomalyEvent[]>({
    url: `${BASE}/anomalies`,
    params: { limit, kind },
  });
}


export function zeroCallProducts(active_product_ids: string[]) {
  return request.post<{ product_id: string; name?: string }[]>({
    url: `${BASE}/products/zero-calls`,
    data: { active_product_ids },
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
