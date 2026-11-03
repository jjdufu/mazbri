/** 计费运维相对时间档（非 OLAP；OLAP 仅用绝对 start/end）。 */
export type OpsRange = "15m" | "1h" | "6h" | "24h" | "7d" | "30d";

export interface StatusCount {
  status: string;
  count: number;
}

export interface FunnelStage {
  stage: string;
  count: number;
  diff: number;
}

export interface BillingFunnelPg {
  since: string;
  stages: FunnelStage[];
  outbox_by_status: StatusCount[];
  queue_by_status: StatusCount[];
  charge_failures: number;
}

export interface ChargeFailureRow {
  id: number;
  queue_id: string;
  tenant_id: string;
  amount: string;
  error_message: string;
  attempt_number: number;
  queue_status: string | null;
  created_at: string | null;
}

export interface DeadLetterRow {
  source: string;
  id: string;
  tenant_id: string;
  retry_count: number;
  error_message: string | null;
  created_at: string | null;
}

export interface BillingFailures {
  limit: number;
  offset: number;
  failures_total: number;
  dead_letters_total: number;
  failures: ChargeFailureRow[];
  dead_letters: DeadLetterRow[];
}

export interface SupplierCostRow {
  supplier_code: string;
  channel_code: string;
  total_calls: number;
  settlement_amount: string;
}
