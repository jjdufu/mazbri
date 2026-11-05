import type { ISODateString, DecimalString } from "./common";
export interface UsageAnalytics {
  tenant_id: string;
  time_range: {
    start: number;
    end: number;
  };
  group_by: "hour" | "day" | "week" | "month";
  analytics: BillingAnalyticsPeriod[];
  top_endpoints: BillingTopEndpoint[];
  status_distribution: StatusDistribution[];
}
export interface BillingAnalyticsPeriod {
  period: string;
  total_requests: number;
  unique_endpoints: number;
  total_units: number;
  total_amount: DecimalString;
  avg_billing_service_duration_ms: number;
  min_billing_service_duration_ms: number;
  max_billing_service_duration_ms: number;
  total_bytes: number;
}
export interface BillingTopEndpoint {
  endpoint: string;
  method: string;
  request_count: number;
  avg_billing_service_duration_ms: number;
  total_cost: DecimalString;
}
export interface StatusDistribution {
  status: number;
  count: number;
}
/** `GET /stats/ops/inventory`：平台规模（不随时间窗变化）。 */
export interface PlatformInventory {
  tenants: { total: number; enabled: number };
  products: { enabled: number };
}
export interface AnalyticsQuery {
  start_time?: number;
  end_time?: number;
  group_by?: "hour" | "day" | "week" | "month";
}
