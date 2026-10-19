import type { ISODateString, DecimalString } from "./common";
export interface Account {
  id: string;
  tenant_id: string;
  balance: DecimalString;
  currency: string;
  status: "active" | "suspended" | "closed";
  created_at: ISODateString;
  updated_at: ISODateString;
}
export interface AccountListResponse {
  accounts: Account[];
  total: number;
  page: number;
  limit: number;
}
export interface AccountListQuery {
  page?: number;
  limit?: number;
  tenant_id?: string;
}
export interface Bill {
  id: string;
  tenant_id: string;
  tenant_name: string;
  tenant_code: string;
  bill_kind?: "customer" | "upstream";
  party_type?: string;
  party_id?: string;
  party_name?: string;
  party_code?: string;
  period_start: ISODateString;
  period_end: ISODateString;
  status: "pending" | "processing" | "paid" | "overdue" | "cancelled";
  subtotal: DecimalString;
  total: DecimalString;
  currency: string;
  total_calls?: number;
  total_data_bytes?: number;
  due_date?: ISODateString;
  paid_at?: ISODateString;
  created_at: ISODateString;
  updated_at: ISODateString;
}
export interface BillLineItem {
  item_id: string;
  description: string;
  billing_type: string;
  quantity: number;
  total_units: number;
  unit_price: DecimalString;
  total: DecimalString;
}
export interface BillDetailResponse {
  bill: Bill;
  items: BillLineItem[];
}
export interface BillListResponse {
  bills: Bill[];
  total: number;
  page: number;
  limit: number;
  total_pages?: number;
}
export interface BillListQuery {
  page?: number;
  limit?: number;
  tenant_id?: string;
  q?: string;
  status?: "pending" | "processing" | "paid" | "overdue" | "cancelled";
  bill_kind?: "customer" | "upstream";

  filters?: string;
}
export interface GenerateBillRequest {
  bill_kind: "customer" | "upstream";
  tenant_code?: string;
  upstream_scope_type?: "channel" | "supplier";
  upstream_scope_id?: string;
  period_start: ISODateString;
  period_end: ISODateString;
}
export interface GenerateBillResponse {
  bill_id: string;
  message: string;
}
export interface UsageSummary {
  tenant_id: string;
  period: "daily" | "weekly" | "monthly";
  start_date: ISODateString;
  end_date: ISODateString;
  total_requests: number;
  total_units: number;
  total_cost: DecimalString;
  breakdown: UsageBreakdown[];
}
export interface UsageBreakdown {
  product_id: string;
  product_name: string;
  request_count: number;
  units: number;
  cost: DecimalString;
}
