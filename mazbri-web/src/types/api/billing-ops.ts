import type {
  FailureQueueKind,
  OpsFailuresQuery,
  OpsRange,
} from "./generated/billing_ops";

export type { FailureQueueKind, OpsFailuresQuery, OpsRange };

/** 漏斗/管道健康模式：平线无失败为 sync_pass_through（展示无堆积），有跌落/失败为 async_funnel。 */
export type BillingFunnelMode = "sync_pass_through" | "async_funnel";

export interface StatusCount {
  status: string;
  count: number;
}

export interface FunnelWindow {
  start: string;
  end: string;
  semantics: "window" | string;
  label: string;
}

export interface FunnelStage {
  key: string;
  name: string;
  count: number;
  /** 相对上一段转化率；首段或缺上一段样本时为 null/省略。 */
  ratio_from_prev?: number | null;
  failed: number;
}

export interface BillingFunnelPg {
  window: FunnelWindow;
  stages: FunnelStage[];
  mode: BillingFunnelMode;
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

export type BillingFailuresQuery = OpsFailuresQuery;

export interface BillingFailures {
  limit: number;
  offset: number;
  failed_offset?: number;
  dlq_offset?: number;
  failures_total: number;
  dead_letters_total: number;
  failures: ChargeFailureRow[];
  dead_letters: DeadLetterRow[];
}
