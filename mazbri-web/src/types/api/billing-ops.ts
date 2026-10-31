import type { OlapRange } from "@/types/api/olap";

export type OpsRange = OlapRange;

export interface StatusCount {
  status: string;
  count: number;
}

export interface FunnelStage {
  stage: string;
  count: number;
  diff: number;
}

export interface ChargeLag {
  samples: number;
  p50_seconds: number | null;
  p95_seconds: number | null;
  max_seconds: number | null;
}

export interface BillingFunnelPg {
  since: string;
  stages: FunnelStage[];
  outbox_by_status: StatusCount[];
  queue_by_status: StatusCount[];
  charge_failures: number;
  charge_lag: ChargeLag;
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
  failures: ChargeFailureRow[];
  dead_letters: DeadLetterRow[];
}

export interface SupplierCostRow {
  supplier_code: string;
  channel_code: string;
  total_calls: number;
  settlement_amount: string;
}
