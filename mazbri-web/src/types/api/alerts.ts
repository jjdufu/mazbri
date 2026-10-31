export type AlertRuleKind = "promql" | "olap";
export type AlertOp = ">" | ">=" | "<" | "<=" | "==" | "!=";
export type AlertSeverity = "critical" | "warning" | "info";
export type AlertStatus = "firing" | "acked" | "resolved";

export interface AlertNotify {
  webhooks?: string[];
}

export interface AlertRule {
  id: string;
  name: string;
  kind: AlertRuleKind;
  expr: string;
  threshold: number;
  op: AlertOp;
  for_seconds: number;
  severity: AlertSeverity;
  labels: Record<string, string>;
  notify: AlertNotify;
  enabled: boolean;
  active_events: number;
  created_at: string;
  updated_at: string;
}

export interface AlertRuleInput {
  name: string;
  kind: AlertRuleKind;
  expr: string;
  threshold: number;
  op: AlertOp;
  for_seconds: number;
  severity: AlertSeverity;
  labels: Record<string, string>;
  notify: { webhooks: string[] };
  enabled: boolean;
}

export interface AlertEvent {
  id: string;
  rule_id: string;
  rule_name: string;
  fingerprint: string;
  status: AlertStatus;
  severity: AlertSeverity;
  labels: Record<string, string>;
  value: number;
  threshold: number;
  op: AlertOp;
  started_at: string;
  resolved_at: string | null;
  acked_by: string | null;
  acked_at: string | null;
  assignee: string | null;
  trace_ids: string[];
  updated_at: string;
}

export interface AlertEventQuery {
  status?: AlertStatus | "active";
  severity?: AlertSeverity;
  rule_id?: string;
  event_id?: string;
  page?: number;
  page_size?: number;
}

export interface AlertEventPage {
  data: AlertEvent[];
  total: number;
  page: number;
  page_size: number;
}

export interface AlertSummary {
  firing: number;
  acked: number;
  critical: number;
  warning: number;
  info: number;
  latest: AlertEvent[];
}
