import type { ISODateString } from "./common";
import type { TenantProductConfig } from "./tenant";
export const PRODUCT_ID_PATTERN = /^8\d{9}$/;
export interface Product {
  id: string;
  product_id: string;
  name: string;
  description?: string;
  timeout_secs: number;
  enabled: boolean;
  cb_pro_enabled: boolean;
  cb_failure_threshold: number;
  cb_success_threshold: number;
  cb_cooldown_secs: number;
  created_at: ISODateString;
  updated_at: ISODateString;
}
export interface ProductListQuery {
  page?: number;
  page_size?: number;
  limit?: number;
  q?: string;

  filters?: string;
}
export interface ProductListResponse {
  products: Product[];
  pagination?: {
    page: number;
    page_size: number;
    total: number;
    total_pages: number;
  };
}
export interface ProductIdReservation {
  id: string;
  product_id: string;
  reserved_at: ISODateString;
  expires_at: ISODateString;
}
export interface CreateProductRequest {
  reservation_id: string;
  name: string;
  description?: string;
  enabled?: boolean;
  timeout_secs?: number;
  cb_pro_enabled?: boolean;
  cb_failure_threshold?: number;
  cb_success_threshold?: number;
  cb_cooldown_secs?: number;
}
export interface UpdateProductRequest {
  name?: string;
  description?: string;
  enabled?: boolean;
  timeout_secs?: number;
  cb_pro_enabled?: boolean;
  cb_failure_threshold?: number;
  cb_success_threshold?: number;
  cb_cooldown_secs?: number;
}
export interface GrantProductRequest {
  tenant_id: string;
}
export interface RevokeProductRequest {
  tenant_id: string;
}
export interface ProductTenantAccess {
  tenant_code: string;
  tenant_id: string;
  enabled: boolean;
  created_at: ISODateString;
}
export interface ProductTenantListResponse {
  tenants: ProductTenantAccess[];
}
export interface AuthorizationDetail {
  id: string;
  tenant_id: string;
  tenant_code: string;
  tenant_name: string;
  product_id: string;
  product_code: string;
  product_name: string;
  enabled: boolean;
  cb_pro_enabled?: boolean;
  unit_price: string;
  currency: string;
  billing_type: string;
  cb_pro_fallback: boolean;
  config: TenantProductConfig;
  created_at: ISODateString;
  updated_at: ISODateString;
}
export interface AuthorizationListQuery {
  page?: number;
  page_size?: number;
  q?: string;

  filters?: string;
}
export interface AuthorizationListResponse {
  authorizations: AuthorizationDetail[];
  pagination: {
    page: number;
    page_size: number;
    total: number;
    total_pages: number;
  };
}
