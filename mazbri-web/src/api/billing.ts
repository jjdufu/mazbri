import request from "@/utils/http";
import type {
  Account,
  AccountListResponse,
  AccountListQuery,
  Bill,
  BillDetailResponse,
  BillLineItem,
  BillListResponse,
  BillListQuery,
  GenerateBillRequest,
  GenerateBillResponse,
  UsageSummary,
} from "@/types/api/billing";
import { API_BASE_PATH, API_PATHS } from "@/config/api";
type BillApiRow = Partial<Bill> & {
  bill_id?: string;
  bill_kind?: string;
  party_type?: string;
  party_id?: string;
  party_code?: string;
  party_name?: string;
  billing_period_start?: string;
  billing_period_end?: string;
  base_amount?: string | number;
  total_amount?: string | number;
  due_at?: string | number;
};
type BillLineItemApiRow = Partial<BillLineItem> & {
  total_amount?: string | number;
};
const toDecimalString = (value: unknown, fallback = "0") =>
  value === null || value === undefined || value === ""
    ? fallback
    : String(value);
const toIsoString = (value: unknown): string | undefined => {
  if (value === null || value === undefined || value === "") return undefined;
  const date =
    typeof value === "number"
      ? new Date(value < 10000000000 ? value * 1000 : value)
      : new Date(String(value));
  return Number.isNaN(date.getTime()) ? undefined : date.toISOString();
};
const normalizeBill = (row: BillApiRow): Bill => {
  const createdAt = toIsoString(row.created_at) ?? new Date(0).toISOString();
  const periodStart =
    toIsoString(row.period_start ?? row.billing_period_start) ?? createdAt;
  const periodEnd =
    toIsoString(row.period_end ?? row.billing_period_end) ?? periodStart;
  return {
    id: String(row.id ?? row.bill_id ?? ""),
    tenant_id: String(row.tenant_id ?? ""),
    tenant_name: String(row.tenant_name ?? row.party_name ?? ""),
    tenant_code: String(row.tenant_code ?? row.party_code ?? ""),
    bill_kind: (row.bill_kind as Bill["bill_kind"]) ?? "customer",
    party_type: row.party_type,
    party_id: row.party_id ? String(row.party_id) : undefined,
    party_name: row.party_name ? String(row.party_name) : undefined,
    party_code: row.party_code ? String(row.party_code) : undefined,
    period_start: periodStart,
    period_end: periodEnd,
    status: row.status ?? "pending",
    subtotal: toDecimalString(row.subtotal ?? row.base_amount),
    total: toDecimalString(row.total ?? row.total_amount),
    currency: row.currency ?? "CNY",
    total_calls: row.total_calls,
    total_data_bytes: row.total_data_bytes,
    due_date: toIsoString(row.due_date ?? row.due_at),
    paid_at: toIsoString(row.paid_at),
    created_at: createdAt,
    updated_at: toIsoString(row.updated_at) ?? createdAt,
  };
};
const normalizeBillLineItems = (items?: BillLineItemApiRow[]): BillLineItem[] =>
  (items ?? []).map((item, index) => ({
    item_id: String(item.item_id ?? `item-${index + 1}`),
    description: String(item.description ?? ""),
    quantity: Number(item.quantity ?? 0),
    total_units: Number(item.total_units ?? 0),
    unit_price: toDecimalString(item.unit_price),
    total: toDecimalString(item.total ?? item.total_amount),
  }));
const normalizeBillListResponse = (
  response: Partial<BillListResponse> & {
    bills?: BillApiRow[];
  },
  params?: BillListQuery,
): BillListResponse => {
  const bills = (response.bills ?? []).map(normalizeBill);
  return {
    bills,
    total: typeof response.total === "number" ? response.total : bills.length,
    page: response.page ?? params?.page ?? 1,
    limit: response.limit ?? params?.limit ?? (bills.length || 10),
    total_pages: response.total_pages,
  };
};
const normalizeBillDetailResponse = (
  response: BillDetailResponse | BillApiRow,
): BillDetailResponse => {
  if ("bill" in response && response.bill) {
    return {
      bill: normalizeBill(response.bill),
      items: normalizeBillLineItems(response.items),
    };
  }
  const bill = response as BillApiRow & {
    items?: BillLineItemApiRow[];
  };
  return {
    bill: normalizeBill(bill),
    items: normalizeBillLineItems(bill.items),
  };
};
export function fetchAccountList(params?: AccountListQuery) {
  return request.get<AccountListResponse>({
    url: `${API_BASE_PATH}${API_PATHS.BILLING}/accounts`,
    params,
  });
}
export function fetchAccountByTenant(tenantId: string) {
  return request.get<Account>({
    url: `${API_BASE_PATH}${API_PATHS.BILLING}/accounts/tenant/${tenantId}`,
  });
}
export async function fetchBillList(params?: BillListQuery) {
  return normalizeBillListResponse(
    await request.get<BillListResponse>({
      url: `${API_BASE_PATH}${API_PATHS.BILLING}/bills`,
      params,
      showErrorMessage: false,
    }),
    params,
  );
}
export async function fetchBillById(id: string): Promise<BillDetailResponse> {
  const response = await request.get<BillDetailResponse | Bill>({
    url: `${API_BASE_PATH}${API_PATHS.BILLING}/bills/${id}`,
    showErrorMessage: false,
  });
  return normalizeBillDetailResponse(response);
}
export async function deleteBill(id: string) {
  await request.del<void>({
    url: `${API_BASE_PATH}${API_PATHS.BILLING}/bills/${id}`,
    showSuccessMessage: true,
  });
}
export function fetchUsageSummary(
  tenantId: string,
  period?: "daily" | "weekly" | "monthly",
) {
  return request.get<UsageSummary>({
    url: `${API_BASE_PATH}${API_PATHS.BILLING}/usage/${tenantId}`,
    params: { period },
  });
}

export function createAdjustment(data: {
  tenant_id: string;
  amount: string;
  direction: "CREDIT" | "DEBIT";
  description: string;
}) {
  return request.post({
    url: `${API_BASE_PATH}${API_PATHS.BILLING}/adjustment`,
    data,
  });
}

export interface ReconciliationDiscrepancy {
  id: string;
  reconciliation_date: string;
  scope_type: string;
  scope_id: string;
  oltp_call_count: number;
  olap_preagg_call_count: number;
  discrepancy_ratio: string;
  status: "ok" | "flagged" | "acknowledged" | "resolved" | string;
  details?: Record<string, unknown> | null;
  resolution_note?: string | null;
  resolved_at?: string | null;
  resolved_by?: string | null;
  created_at: string;
}

export interface ReconciliationDiscrepancyListQuery {
  from?: string;
  to?: string;
  status?: string;
  scope_type?: string;
  scope_id?: string;

  q?: string;

  filters?: string;
  page?: number;
  page_size?: number;
}

export interface ReconciliationDiscrepancyListResponse {
  items: ReconciliationDiscrepancy[];
  total: number;
  page: number;
  page_size: number;
  total_pages: number;
}

export interface ReconciliationAuditResult {
  discrepancy_id: string;
  reconciliation_date: string;
  scope_type: string;
  scope_id: string;
  oltp_call_count: number;
  olap_preagg_call_count: number;
  loss_ratio: string;
  loss_ratio_threshold: string;
  status: string;
}
export function generateBill(data: GenerateBillRequest) {
  return request.post<GenerateBillResponse>({
    url: `${API_BASE_PATH}${API_PATHS.BILLING}/bills/generate`,
    data,
    showSuccessMessage: false,
    showErrorMessage: false,
  });
}

export function generateBillAsync(data: GenerateBillRequest) {
  return request.post<{ bill_id: string; total_channels?: number; total_calls?: number; total_amount?: string }>({
    url: `${API_BASE_PATH}${API_PATHS.BILLING}/bills/generate-async`,
    data,
    showSuccessMessage: false,
    showErrorMessage: false,
  });
}

export function runReconciliationAudit(data: {
  reconciliation_date: string;
  scope_type?: string;
  scope_id?: string;
}) {
  return request.post<ReconciliationAuditResult>({
    url: `${API_BASE_PATH}${API_PATHS.BILLING}/reconciliation/audit`,
    data,
  });
}

export function fetchReconciliationDiscrepancies(
  params?: ReconciliationDiscrepancyListQuery,
) {
  return request.get<ReconciliationDiscrepancyListResponse>({
    url: `${API_BASE_PATH}${API_PATHS.BILLING}/reconciliation/discrepancies`,
    params,
  });
}


export interface ReconciliationSchedule {
  enabled: boolean;
  interval_secs: number;
  use_previous_day: boolean;
}

export interface ReconciliationAlert {
  on_flagged: boolean;
  webhook_url: string;
}

export interface ReconciliationSources {
  include_amount_recon: boolean;
  include_raw_log_source: boolean;
}

export interface ReconciliationSettings {
  schedule: ReconciliationSchedule;
  alert: ReconciliationAlert;
  sources: ReconciliationSources;
  loss_ratio_threshold: number;
  default_scope_type: string;
  default_scope_id: string;
  warn_on_open_discrepancies_before_bill: boolean;
  last_scheduled_run_at?: string | null;
}

export function fetchReconciliationSettings() {
  return request.get<ReconciliationSettings>({
    url: `${API_BASE_PATH}${API_PATHS.BILLING}/reconciliation/settings`,
  });
}

export function updateReconciliationSettings(data: ReconciliationSettings) {
  return request.put<ReconciliationSettings>({
    url: `${API_BASE_PATH}${API_PATHS.BILLING}/reconciliation/settings`,
    data,
    showSuccessMessage: true,
  });
}

export function updateReconciliationDiscrepancy(
  id: string,
  data: { status: "acknowledged" | "resolved"; resolution_note?: string },
) {
  return request.patch<ReconciliationDiscrepancy>({
    url: `${API_BASE_PATH}${API_PATHS.BILLING}/reconciliation/discrepancies/${id}`,
    data,
    showSuccessMessage: true,
  });
}


export async function deleteReconciliationDiscrepancy(id: string) {
  await request.del<{ id: string; message: string }>({
    url: `${API_BASE_PATH}${API_PATHS.BILLING}/reconciliation/discrepancies/${id}`,
    showSuccessMessage: true,
  });
}


export interface BillAdjustment {
  id: string;
  bill_id?: string | null;
  tenant_id?: string | null;
  adjustment_type: string;
  amount: string;
  reason: string;
  reference_period_start: string;
  reference_period_end: string;
  status: "posted" | "void" | string;
  operator_id?: string | null;
  created_at: string;
}

export function fetchBillAdjustments(billId: string) {
  return request.get<{ items: BillAdjustment[] }>({
    url: `${API_BASE_PATH}${API_PATHS.BILLING}/bills/${billId}/adjustments`,
  });
}

export function createBillAdjustment(
  billId: string,
  data: { adjustment_type: string; amount: string; reason: string },
) {
  return request.post<BillAdjustment>({
    url: `${API_BASE_PATH}${API_PATHS.BILLING}/bills/${billId}/adjustments`,
    data,
    showSuccessMessage: true,
  });
}

export function voidBillAdjustment(adjustmentId: string) {
  return request.post<BillAdjustment>({
    url: `${API_BASE_PATH}${API_PATHS.BILLING}/adjustments/${adjustmentId}/void`,
    showSuccessMessage: true,
  });
}

export function fetchGlobalBillAdjustments(params?: {
  bill_id?: string;
  tenant_id?: string;
  status?: string;
  page?: number;
  page_size?: number;
}) {
  return request.get<{
    items: BillAdjustment[];
    total: number;
    page: number;
    page_size: number;
    total_pages: number;
  }>({
    url: `${API_BASE_PATH}${API_PATHS.BILLING}/adjustments`,
    params,
  });
}


export function updateBillStatus(
  id: string,
  status: "pending" | "processing" | "paid" | "overdue" | "cancelled",
) {
  return request.post<BillDetailResponse>({
    url: `${API_BASE_PATH}${API_PATHS.BILLING}/bills/${id}/status`,
    data: { status },
    showSuccessMessage: true,
  });
}

