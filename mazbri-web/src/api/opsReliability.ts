import request from "@/utils/http";
import { API_BASE_PATH } from "@/config/api";

const BASE = `${API_BASE_PATH}/stats/ops`;

export interface LoadtestRun {
  id: string;
  started_at: string;
  ended_at: string;
  scenario: string;
  products: number;
  requests: number;
  p50: number;
  p95: number;
  p99: number;
  internal_p99: number;
  notes: string;
}

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

export interface CapacitySnapshot {
  detected_at: string | null;
  holt_next: number | null;
  qps_cap: number | null;
  days_to_cap: number | null;
  detail: Record<string, unknown>;
}

export function listLoadtests(limit = 20) {
  return request.get<LoadtestRun[]>({ url: `${BASE}/loadtests`, params: { limit } });
}

export function listAnomalies(limit = 50, kind?: string) {
  return request.get<AnomalyEvent[]>({
    url: `${BASE}/anomalies`,
    params: { limit, kind },
  });
}

export function getCapacity() {
  return request.get<CapacitySnapshot>({ url: `${BASE}/capacity` });
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
