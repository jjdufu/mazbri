import request from "@/utils/http";
import { API_BASE_PATH } from "@/config/api";
export interface CallbackSubscription {
  id: string;
  tenant_code: string;
  tenant_id: string;
  product_code: string;
  callback_url: string;
  is_active: boolean;
}
export interface CreateSubscriptionRequest {
  tenant_code: string;
  product_code: string;
  callback_url: string;
  secret_key: string;
  retry_policy?: Record<string, unknown>;
}
export interface CallbackDeliveryLog {
  id: string;
  subscription_id: string;
  event_id: string;
  callback_url: string;
  status: string;
  retry_count: number;
  next_retry_at?: string | null;
  last_response_code?: number | null;
  last_error?: string | null;
  created_at: string;
  updated_at: string;
}
export interface CallbackListQuery {
  q?: string;

  filters?: string;
  page?: number;
  page_size?: number;
}

export interface PaginatedCallbackSubscriptions {
  data: CallbackSubscription[];
  total: number;
  page: number;
  page_size: number;
}

export interface PaginatedCallbackDeliveries {
  data: CallbackDeliveryLog[];
  total: number;
  page: number;
  page_size: number;
}

export async function fetchCallbackSubscriptions(params?: CallbackListQuery) {
  return request.get<PaginatedCallbackSubscriptions>({
    url: `${API_BASE_PATH}/callbacks/subscriptions`,
    params,
    showErrorMessage: false,
  });
}
export function createCallbackSubscription(
  data: CreateSubscriptionRequest,
  options?: {
    signal?: AbortSignal;
    showSuccessMessage?: boolean;
  },
) {
  return request.post<CallbackSubscription>({
    url: `${API_BASE_PATH}/callbacks/subscriptions`,
    params: data,
    showSuccessMessage: options?.showSuccessMessage ?? true,
    signal: options?.signal,
  });
}
export function deleteCallbackSubscription(id: string) {
  return request.del({
    url: `${API_BASE_PATH}/callbacks/subscriptions/${id}`,
  });
}
export async function fetchCallbackDeliveries(params?: CallbackListQuery) {
  return request.get<PaginatedCallbackDeliveries>({
    url: `${API_BASE_PATH}/callbacks/deliveries`,
    params,
    showErrorMessage: false,
  });
}
export function retryCallbackDelivery(id: string) {
  return request.post({
    url: `${API_BASE_PATH}/callbacks/deliveries/${id}/retry`,
  });
}
