import request from "@/utils/http";
import { API_BASE_PATH } from "@/config/api";
export interface Pagination {
  page: number;
  page_size: number;
  total: number;
  total_pages: number;
}
export interface RouteRule {
  id: string;
  tenant_id?: string | null;
  product_id: string;
  priority: number;
  status: string;
  metadata: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}
export interface Channel {
  id: string;
  supplier_id: string;
  supplier_code: string;
  supplier_name: string;
  channel_code: string;
  name: string;
  path_prefix: string;
  cost_price: string;
  qps_limit: number;
  status: string;
  config: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}
export interface RouteBinding {
  id: string;
  route_rule_id: string;
  channel_id: string;
  channel_order: number;
  switch_condition: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}
export interface RouteRuleListResponse {
  route_rules: RouteRule[];
  pagination: Pagination;
}
export interface ChannelListResponse {
  channels: Channel[];
  pagination: Pagination;
}
export interface RouteBindingListResponse {
  bindings: RouteBinding[];
}
export interface CreateRouteRuleRequest {
  tenant_id?: string | null;
  product_id: string;
  priority?: number;
  status?: string;
  metadata?: Record<string, unknown>;
  channel_id: string;
  timeout_ms?: number;
  max_retries?: number;
}
export interface UpdateRouteRuleRequest {
  product_id?: string;
  priority?: number;
  status?: string;
  metadata?: Record<string, unknown>;
}
export interface CreateRouteBindingRequest {
  channel_id: string;
  channel_order: number;
  switch_condition?: Record<string, unknown>;
}
export interface UpdateRouteBindingRequest {
  channel_id?: string;
  channel_order?: number;
  switch_condition?: Record<string, unknown>;
}
export async function fetchRouteRules(params?: {
  page?: number;
  page_size?: number;
  q?: string;

  search?: string;
  status?: string;
  tenant_id?: string;
  product_id?: string;

  filters?: string;
}) {
  const { search, q, ...rest } = params ?? {};
  return request.get<RouteRuleListResponse>({
    url: `${API_BASE_PATH}/route-rules`,
    params: {
      ...rest,
      q: q ?? search,
    },
    showErrorMessage: false,
  });
}
export function createRouteRule(data: CreateRouteRuleRequest) {
  return request.post<RouteRule>({
    url: `${API_BASE_PATH}/route-rules`,
    data,
  });
}
export function updateRouteRule(id: string, data: UpdateRouteRuleRequest) {
  return request.put<RouteRule>({
    url: `${API_BASE_PATH}/route-rules/${id}`,
    data,
  });
}
export function deleteRouteRule(id: string) {
  return request.del({
    url: `${API_BASE_PATH}/route-rules/${id}`,
  });
}
export async function fetchChannels(params?: {
  page?: number;
  page_size?: number;
  q?: string;

  search?: string;
  status?: string;

  filters?: string;
}) {
  const { search, q, ...rest } = params ?? {};
  return request.get<ChannelListResponse>({
    url: `${API_BASE_PATH}/channels`,
    params: {
      ...rest,
      q: q ?? search,
    },
    showErrorMessage: false,
  });
}
export function fetchChannelById(id: string) {
  return request.get<Channel>({
    url: `${API_BASE_PATH}/channels/${id}`,
  });
}
export async function fetchNextChannelCode() {
  return request.get<{ channel_code: string }>({
    url: `${API_BASE_PATH}/channels/next-code`,
    showErrorMessage: false,
  });
}
export interface CreateChannelRequest {
  supplier_id: string;
  channel_code: string;
  name: string;
  path_prefix: string;
  cost_price?: string;
  qps_limit?: number;
  config?: Record<string, unknown>;
}
export interface UpdateChannelRequest extends Partial<CreateChannelRequest> {
  status?: string;
}
export function createChannel(data: CreateChannelRequest) {
  return request.post<Channel>({
    url: `${API_BASE_PATH}/channels`,
    data,
  });
}
export function updateChannel(id: string, data: UpdateChannelRequest) {
  return request.put<Channel>({
    url: `${API_BASE_PATH}/channels/${id}`,
    data,
  });
}
export function deleteChannel(id: string) {
  return request.del({
    url: `${API_BASE_PATH}/channels/${id}`,
  });
}
export async function fetchRouteBindings(routeRuleId: string) {
  return request.get<RouteBindingListResponse>({
    url: `${API_BASE_PATH}/route-rules/${routeRuleId}/bindings`,
    showErrorMessage: false,
  });
}
export function createRouteBinding(
  routeRuleId: string,
  data: CreateRouteBindingRequest,
) {
  return request.post<RouteBinding>({
    url: `${API_BASE_PATH}/route-rules/${routeRuleId}/bindings`,
    data,
  });
}
export function updateRouteBinding(
  id: string,
  data: UpdateRouteBindingRequest,
) {
  return request.put<RouteBinding>({
    url: `${API_BASE_PATH}/route-channel-bindings/${id}`,
    data,
  });
}
export function deleteRouteBinding(id: string) {
  return request.del({
    url: `${API_BASE_PATH}/route-channel-bindings/${id}`,
  });
}
export function emergencySwitchRoute(
  routeRuleId: string,
  data: {
    channel_id: string;
    reason: string;
  },
) {
  return request.post({
    url: `${API_BASE_PATH}/route-rules/${routeRuleId}/emergency-switch`,
    data,
  });
}
