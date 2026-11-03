import request from "@/utils/http";
import { API_BASE_PATH } from "@/config/api";
import type {
  BillingFailures,
  BillingFunnelPg,
  OpsRange,
  SupplierCostRow,
} from "@/types/api/billing-ops";

const BASE_PATH = `${API_BASE_PATH}/billing/ops`;

export type CostStatus = "ok" | "unavailable" | "partial";

export interface MoneyWindow {
  start: string;
  end: string;
  semantics: string;
  label: string;
}

export async function fetchBillingFunnel(range: OpsRange) {
  return request.get<BillingFunnelPg>({
    url: `${BASE_PATH}/funnel`,
    params: { range },
    showErrorMessage: false,
  });
}

export async function fetchBillingFailures(
  range: OpsRange,
  limit = 50,
  offset = 0,
) {
  return request.get<BillingFailures>({
    url: `${BASE_PATH}/failures`,
    params: { range, limit, offset },
    showErrorMessage: false,
  });
}

export async function fetchSupplierCosts(range: OpsRange) {
  return request.get<SupplierCostRow[]>({
    url: `${BASE_PATH}/supplier-costs`,
    params: { range },
    showErrorMessage: false,
  });
}

export interface FinanceOverview {
  window: MoneyWindow;
  days: Array<{
    day: string;
    revenue: string;
    cost: string;
    cost_status: CostStatus;
    margin: string | null;
    margin_rate: number | null;
  }>;
  revenue_total: string;
  cost_total: string;
  cost_status: CostStatus;
  margin_total: string | null;
  margin_rate: number | null;
  inverted_days: string[];
}

export interface FinanceRankRow {
  key: string;
  display_name: string;
  tenant_code?: string | null;
  revenue: string;
  cost: string;
  cost_status: CostStatus;
  margin: string | null;
}

export interface FinanceRanks {
  window: MoneyWindow;
  by_tenant: FinanceRankRow[];
  by_product: FinanceRankRow[];
  by_supplier: FinanceRankRow[];
}

export interface BalanceRow {
  tenant_id: string;
  tenant_code: string | null;
  tenant_name: string | null;
  balance: string;
  credit_limit: string;
  available_balance: string;
  status: string;
  recent_recharge: string;
  avg_daily_spend: string;
  days_remaining: number | null;
}

export interface ReconcileCalendarDay {
  day: string;
  open_count: number;
  resolved_count: number;
  error_count: number;
  status: string;
}

export async function fetchFinanceOverview(days = 30) {
  return request.get<FinanceOverview>({
    url: `${BASE_PATH}/finance/overview`,
    params: { days },
    showErrorMessage: false,
  });
}

export async function fetchFinanceRanks(days = 30, limit = 10) {
  return request.get<FinanceRanks>({
    url: `${BASE_PATH}/finance/ranks`,
    params: { days, limit },
    showErrorMessage: false,
  });
}

export async function fetchFinanceBalances(limit = 50) {
  return request.get<BalanceRow[]>({
    url: `${BASE_PATH}/finance/balances`,
    params: { limit },
    showErrorMessage: false,
  });
}

export async function fetchReconcileCalendar(days = 30) {
  return request.get<ReconcileCalendarDay[]>({
    url: `${BASE_PATH}/reconcile/calendar`,
    params: { days },
    showErrorMessage: false,
  });
}
