import request from "@/utils/http";
import { API_BASE_PATH } from "@/config/api";
import type {
  BillingFailures,
  BillingFunnelPg,
  OpsRange,
  SupplierCostRow,
} from "@/types/api/billing-ops";

const BASE_PATH = `${API_BASE_PATH}/billing/ops`;

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
