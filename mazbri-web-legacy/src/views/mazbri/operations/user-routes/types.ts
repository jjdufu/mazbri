

export type RouteScope = "default" | "tenant";

export interface RouteFormState {
  scope: RouteScope;
  tenant_id: string;
  product_id: string;
  priority: number;
  status: string;
  route_purpose: string;
  effective_mode: "immediate" | "window";
  effective_range: string[];
  channel_id: string;
  timeout_ms: number;
  max_retries: number;
}

export interface BindingFormState {
  channel_id: string;
  channel_order: number;
  timeout_ms: number;
  max_retries: number;
  failover_enabled: boolean;
  failure_types: string[];
  description: string;
}

export type RouteImportRow = Record<string, unknown> & {
  key: string;
  product_code: string;
  tenant_code: string;
  channel_code: string;
  priority: number;
  status: string;
  route_purpose: string;
};
