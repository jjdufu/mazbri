import type { TenantStatus, ISODateString, DecimalString } from "./common";
export type { TenantStatus } from "./common";
export interface Tenant {
  tenant_code: string;
  tenant_id: string;
  name: string;
  status: TenantStatus;
  contact_person?: string;
  contact_email?: string;
  contact_phone?: string;
  token_expiration_secs?: number;
  created_at: ISODateString;
  updated_at: ISODateString;
}
export interface TenantListResponse {
  tenants: Tenant[];
  pagination: {
    page: number;
    page_size: number;
    total: number;
    total_pages: number;
  };
}
export interface TenantListQuery {
  page?: number;
  page_size?: number;
  q?: string;
  status?: TenantStatus;
}
export interface CreateTenantRequest {
  tenant_code: string;
  tenant_id: string;
  name: string;
  contact_person?: string;
  contact_phone?: string;
  contact_email?: string;
  token_expiration_secs?: number;
}
export interface UpdateTenantRequest {
  name?: string;
  contact_person?: string;
  contact_email?: string;
  contact_phone?: string;
  status?: TenantStatus;
}
export interface SetTenantStatusRequest {
  status: TenantStatus;
}
export interface TenantAccount {
  tenant_id: string;
  balance: DecimalString;
  credit_limit?: DecimalString;
  available_balance?: DecimalString;
  status: "active" | "suspended" | "closed";

  credit_tier?: string;

  billing_cycle?: string;
  currency: string;
  created_at: ISODateString;
  updated_at: ISODateString;
  actions?: {
    recharge: boolean;
    refund: boolean;
  };
}
export interface RechargeRequest {
  amount: number;
  description?: string;
}
export interface RechargeResponse {
  transaction_id: string;
  new_balance: DecimalString;
}
export interface RefundRequest {
  amount: number;
  reason?: string;
  bill_id?: string;
}
export interface RefundResponse {
  success: boolean;
  transaction_id: string;
  balance_before: DecimalString;
  balance_after: DecimalString;
}
export interface Transaction {
  id: string;
  transaction_type: "credit" | "refund" | "charge" | "adjustment" | "payment";
  amount: DecimalString;
  balance_before: DecimalString;
  balance_after: DecimalString;
  description?: string;
  payment_method?: string;
  operator_id?: string;
  operator_name?: string;
  created_at: ISODateString;
}
export interface TransactionListQuery {
  page?: number;
  limit?: number;
  transaction_type?: "credit" | "refund" | "charge" | "adjustment" | "payment";
}
export interface TransactionListResponse {
  transactions: Transaction[];
  total: number;
  page: number;
  limit: number;
}

export interface TenantRateLimitConfig {
  tenant_id: string;

  max_concurrent: number;

  tenant_qps?: number;
  max_bandwidth_mbps: number;

  api_limits: Record<string, number>;
  burst_size: number;
  client_ip_qps: number;
  client_ip_burst: number;
  enabled: boolean;

  enforcement_model?: string;

  cluster_note?: string;
  created_at?: ISODateString;
  updated_at?: ISODateString;
  message?: string;
}


export type RateLimitCommandType =
  | "deny_until"
  | "quota_correction"
  | "refresh_policy";

export interface RateLimitControlCommand {
  id: string;
  tenant_id: string;
  tenant_code: string;
  policy_version: number;
  command_type: RateLimitCommandType | string;
  command_payload: Record<string, unknown>;
  effective_at: ISODateString;
  expires_at?: ISODateString | null;
  created_by?: string | null;
  created_at: ISODateString;
  acknowledged_by: Record<string, unknown>;
  status: "active" | "expired" | string;
}

export interface CreateRateLimitCommandRequest {
  command_type: RateLimitCommandType;
  deny_until_unix?: number;
  deny_for_secs?: number;
  quota_multiplier?: number;
  expires_in_secs?: number;
  reason?: string;
}

export interface RateLimitCommandListResponse {
  commands: RateLimitControlCommand[];
  total: number;
}

export interface UpdateTenantRateLimitConfigRequest {

  max_concurrent: number;
  max_bandwidth_mbps: number;

  api_limits: Record<string, number>;
  burst_size: number;
  client_ip_qps: number;
  client_ip_burst: number;
  enabled: boolean;
  expected_updated_at?: ISODateString;
}
export interface TenantCacheConfig {
  tenant_id: string;
  enabled: boolean;
  default_ttl: number;
  max_cache_size: number;
  api_ttl: Record<string, number>;
  tenant_isolated: boolean;
  warmup_enabled: boolean;
  warmup_keys: string[];
  created_at?: ISODateString;
  updated_at?: ISODateString;
  message?: string;
}
export interface UpdateTenantCacheConfigRequest {
  enabled: boolean;
  default_ttl: number;
  max_cache_size: number;
  api_ttl: Record<string, number>;
  tenant_isolated: boolean;
  warmup_enabled: boolean;
  warmup_keys: string[];
  expected_updated_at?: ISODateString;
}
export interface CBProPolicyConfig {
  mode: "disabled" | "fallback_only" | "cost_saving";
  base_ttl_secs: number;
  min_ttl_secs: number;
  max_ttl_secs: number;
  ttl_extend_step_secs: number;
  ttl_extend_multiplier: number;
  validate_every_hits: number;
  max_validate_interval_secs: number;
  prevalidate_before_expiry_secs: number;
  latency_simulation_enabled: boolean;
  default_simulated_latency_ms: number;
  latency_jitter_min_ms: number;
  latency_jitter_max_ms: number;
  max_simulated_delay_ms: number;
  cacheable_methods: string[];
  allow_body_cache: boolean;
  ignore_query_keys: string[];
  ignore_json_paths: string[];
  shadow_validate_enabled: boolean;
  background_analysis_enabled: boolean;
}
export interface TenantProductConfig {
  unit_price: DecimalString;
  currency: string;
  billing_type: string;
  cb_pro_fallback: boolean;
  cbpro: CBProPolicyConfig;
  request_codec: PayloadCodec;
  response_codec: PayloadCodec;
  codec_enabled: boolean;
  codec_version: number;
}
export interface UpdateTenantProductConfigRequest extends TenantProductConfig {
  expected_updated_at?: ISODateString;
}
export interface TenantProduct {
  product_id: string;
  name: string;
  description?: string;
  cb_pro_enabled?: boolean;
  config: TenantProductConfig;
  granted_at: ISODateString;
  updated_at?: ISODateString;
}
export interface TenantProductListResponse {
  tenant_id: string;
  products: TenantProduct[];
  total: number;
  actions?: {
    grant: boolean;
    revoke: boolean;
  };
}
export interface WhitelistEntry {
  id: string;
  ip_address: string;
  cidr: number;
  description?: string;
  is_active: boolean;
  created_at: ISODateString;
  updated_at: ISODateString;
}
export interface WhitelistListResponse {
  entries: WhitelistEntry[];
}
export interface AddToWhitelistRequest {
  ip_addresses: string[];
  description?: string;
}
export interface AddToWhitelistResponse {
  added: Array<{
    id: string;
    ip_address: string;
    cidr: number;
    description?: string;
    created_at: ISODateString;
  }>;
  errors: Array<{
    ip_address: string;
    error: string;
  }>;
  total_added: number;
  total_errors: number;
}
export interface RemoveFromWhitelistResponse {
  message: string;
  tenant_id: string;
  ip_address: string;
}
export type TenantEncryptionMode = "none" | "request" | "full";
export type TenantCryptoKeyStatus =
  | "pending"
  | "active"
  | "deprecated"
  | "disabled"
  | "compromised";
export type PayloadCodec = "json" | "fury";
export interface TenantCryptoProfile {
  tenant_id: string;
  encryption_mode: TenantEncryptionMode;
  require_signature: boolean;
  request_algorithm: string;
  response_algorithm: string;
  signature_algorithm: string;
  current_key_version?: string;
  nonce_ttl_seconds: number;
  clock_skew_seconds: number;
  request_codec: PayloadCodec;
  response_codec: PayloadCodec;
  codec_enabled: boolean;
  codec_version: number;
  config: Record<string, unknown>;
  created_at: ISODateString;
  updated_at: ISODateString;
}
export interface UpdateTenantCryptoProfileRequest {
  encryptionMode: TenantEncryptionMode;
  requireSignature: boolean;
  nonceTtlSeconds: number;
  clockSkewSeconds: number;
  currentKeyVersion?: string;
  requestCodec: PayloadCodec;
  responseCodec: PayloadCodec;
  codecEnabled: boolean;
  codecVersion: number;
}
export interface TenantCryptoKey {
  id: string;
  tenant_id: string;
  key_version: string;
  secret_key_digest: string;
  wrapping_key_id: string;
  wrapping_algorithm: string;
  data_algorithm: string;
  signature_algorithm: string;
  status: TenantCryptoKeyStatus;
  not_before?: ISODateString;
  expires_at?: ISODateString;
  activated_at?: ISODateString;
  deprecated_at?: ISODateString;
  disabled_at?: ISODateString;
  last_used_at?: ISODateString;
  created_at: ISODateString;
  updated_at: ISODateString;
  metadata: Record<string, unknown>;
}
export interface TenantCryptoKeyListResponse {
  keys: TenantCryptoKey[];
}
export interface TenantCryptoKeyCreateResponse {
  keyVersion: string;
  tenantSecretKey: string;
  algorithm: string;
  signatureAlgorithm: string;
  shownOnce: boolean;
  status: TenantCryptoKeyStatus;
}
