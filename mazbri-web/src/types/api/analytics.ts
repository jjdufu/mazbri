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
export interface PlatformStats {
  time_range: {
    start: number;
    end: number;
  };
  tenant_stats: {
    total_tenants: number;
    active_tenants: number;
  };
  product_stats: {
    total_products: number;
  };
  usage_stats: {
    total_requests: number;
    active_tenants_usage: number;
    total_bytes: number;
    avg_customer_total_ms: number;
  };
  top_tenants: TopTenant[];
  top_products: TopProduct[];
}
export interface TopTenant {
  tenant_id: string;
  tenant_name: string;
  request_count: number;
  total_cost: DecimalString;
  total_bytes: number;
}
export interface TopProduct {
  product_endpoint: string;
  request_count: number;
  avg_service_total_ms: number;
  total_cost: DecimalString;
}
export interface DashboardMetrics {
  total_requests_today: number;
  total_requests_week: number;
  total_requests: number;
  active_tenants: number;
  top_endpoints: BillingTopEndpoint[];
  requests_by_day: {
    date: string;
    count: number;
  }[];
}
export interface AnalyticsQuery {
  start_time?: number;
  end_time?: number;
  group_by?: "hour" | "day" | "week" | "month";
}
