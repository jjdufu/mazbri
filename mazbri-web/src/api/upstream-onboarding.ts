import request from "@/utils/http";
import { API_BASE_PATH } from "@/config/api";

export interface OnboardingRequest {
  supplier: {
    supplier_code: string;
    name: string;
    upstream_type?: string;
    upstream_url: string;
    status?: string;
    config?: Record<string, unknown>;
  };
  channel: {
    channel_code: string;
    name: string;
    path_prefix?: string;
    cost_price?: string;
    qps_limit?: number;
  };
  route: {
    product_ref: { product_id: string };
    tenant_id?: string | null;
    priority?: number;
    weight?: number;
    version_tag?: string;
    switch_condition?: Record<string, unknown>;
  };
  authorization?: {
    tenant_ids?: string[];
    enabled?: boolean;
  };
  options?: {
    dry_run?: boolean;
    fail_if_route_exists?: boolean;
  };
}

export interface OnboardingResult {
  supplier_id?: string;
  channel_id?: string;
  route_rule_id?: string;
  binding_id?: string;
  route_version?: number;
  warnings: string[];
  dry_run: boolean;
  plan?: {
    supplier_code: string;
    channel_code: string;
    product_external_id: string;
    product_uuid: string;
    upstream_type: string;
    upstream_url: string;
    actions: string[];
  };
}

export interface RouteVersionInfo {
  version: number;
  updated_at: string;
}

export function createOnboarding(data: OnboardingRequest) {
  return request.post<OnboardingResult>({
    url: `${API_BASE_PATH}/upstreams/onboarding`,
    data,
  });
}

export function dryRunOnboarding(data: OnboardingRequest) {
  return request.post<OnboardingResult>({
    url: `${API_BASE_PATH}/upstreams/onboarding/dry-run`,
    data,
  });
}

export function getGatewayRouteVersion() {
  return request.get<RouteVersionInfo>({
    url: `${API_BASE_PATH}/gateway/route-version`,
    showErrorMessage: false,
  });
}
