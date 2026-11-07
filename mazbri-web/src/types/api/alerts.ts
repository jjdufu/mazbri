export type AlertRuleKind = "promql" | "olap";
export type AlertOp = ">" | ">=" | "<" | "<=" | "==" | "!=";
export type AlertSeverity = "critical" | "warning" | "info";
export type AlertStatus = "firing" | "resolved";

/** 规则通知：enabled=false 只落库；true 时 emails/webhooks 至少一项。 */
export interface AlertNotify {
  enabled: boolean;
  emails: string[];
  webhooks: string[];
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
  notify: AlertNotify;
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
  trace_ids: string[];
  silenced: boolean;
  inhibited: boolean;
  updated_at: string;
}

export interface AlertEventQuery {
  status?: AlertStatus;
  severity?: AlertSeverity;
  rule_id?: string;
  event_id?: string;
  page?: number;
  page_size?: number;
  /** 默认 false；仅超管诊断可 true。业务页不要传。 */
  include_seed?: boolean;
}

export interface AlertEventPage {
  data: AlertEvent[];
  total: number;
  page: number;
  page_size: number;
}

export interface AlertSummary {
  firing: number;
  critical: number;
  warning: number;
  info: number;
  latest: AlertEvent[];
}
