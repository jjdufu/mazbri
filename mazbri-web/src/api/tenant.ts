import request from "@/utils/http";
import type {
  Tenant,
  TenantListResponse,
  TenantListQuery,
  CreateTenantRequest,
  UpdateTenantRequest,
  SetTenantEnabledRequest,
  TenantAccount,
  RechargeRequest,
  RechargeResponse,
  RefundRequest,
  RefundResponse,
  TransactionListQuery,
  TransactionListResponse,
  TenantProduct,
  TenantProductConfig,
  CBProPolicyConfig,
  UpdateTenantProductConfigRequest,
  TenantProductListResponse,
  TenantRateLimitConfig,
  TenantCacheConfig,
  UpdateTenantRateLimitConfigRequest,
  UpdateTenantCacheConfigRequest,
  RateLimitCommandListResponse,
  CreateRateLimitCommandRequest,
  RateLimitControlCommand,
  WhitelistListResponse,
  AddToWhitelistRequest,
  AddToWhitelistResponse,
  RemoveFromWhitelistResponse,
  TenantCryptoProfile,
  UpdateTenantCryptoProfileRequest,
  TenantCryptoKeyListResponse,
  TenantCryptoKeyCreateResponse,
  TenantCryptoKey,
} from "@/types/api/tenant";
import type {
  ApiKey,
  ApiKeyListResponse,
  CreateApiKeyRequest,
  CreateApiKeyResponse,
} from "@/types/api/api-key";
import { API_BASE_PATH, API_PATHS } from "@/config/api";
import { requireTenantCode } from "@/utils/tenant-code";
const BASE_PATH = `${API_BASE_PATH}${API_PATHS.TENANTS}`;
const PRODUCT_BASE_PATH = `${API_BASE_PATH}${API_PATHS.PRODUCTS}`;

function tenantPath(code: unknown, ...segments: string[]): string {
  const safe = requireTenantCode(code);
  if (segments.length === 0) {
    return `${BASE_PATH}/${safe}`;
  }
  return `${BASE_PATH}/${safe}/${segments.join("/")}`;
}





export const DEFAULT_CBPRO_POLICY_CONFIG: CBProPolicyConfig = {
  mode: "fallback_only",
  base_ttl_secs: 300,
  min_ttl_secs: 60,
  max_ttl_secs: 3600,
  ttl_extend_step_secs: 300,
  ttl_extend_multiplier: 1.5,
  validate_every_hits: 10,
  max_validate_interval_secs: 300,
  prevalidate_before_expiry_secs: 30,
  latency_simulation_enabled: false,
  default_simulated_latency_ms: 120,
  latency_jitter_min_ms: 0,
  latency_jitter_max_ms: 20,
  max_simulated_delay_ms: 1000,
  cacheable_methods: ["GET", "POST"],
  allow_body_cache: true,
  ignore_query_keys: [],
  ignore_json_paths: [],
  shadow_validate_enabled: false,
  background_analysis_enabled: false,
};
export const DEFAULT_TENANT_PRODUCT_CONFIG: TenantProductConfig = {
  unit_price: "0.001",
  currency: "CNY",
  cb_pro_fallback: false,
  cbpro: DEFAULT_CBPRO_POLICY_CONFIG,
};
export function createDefaultCBProPolicyConfig(): CBProPolicyConfig {
  return {
    ...DEFAULT_CBPRO_POLICY_CONFIG,
    cacheable_methods: [...DEFAULT_CBPRO_POLICY_CONFIG.cacheable_methods],
    ignore_query_keys: [...DEFAULT_CBPRO_POLICY_CONFIG.ignore_query_keys],
    ignore_json_paths: [...DEFAULT_CBPRO_POLICY_CONFIG.ignore_json_paths],
  };
}
export function createDefaultTenantProductConfig(): TenantProductConfig {
  return {
    ...DEFAULT_TENANT_PRODUCT_CONFIG,
    cbpro: createDefaultCBProPolicyConfig(),
  };
}
function mergeCBProPolicyConfig(
  config?: Partial<CBProPolicyConfig>,
): CBProPolicyConfig {
  return {
    ...createDefaultCBProPolicyConfig(),
    ...config,
    cacheable_methods: config?.cacheable_methods
      ? [...config.cacheable_methods]
      : [...DEFAULT_CBPRO_POLICY_CONFIG.cacheable_methods],
    ignore_query_keys: config?.ignore_query_keys
      ? [...config.ignore_query_keys]
      : [...DEFAULT_CBPRO_POLICY_CONFIG.ignore_query_keys],
    ignore_json_paths: config?.ignore_json_paths
      ? [...config.ignore_json_paths]
      : [...DEFAULT_CBPRO_POLICY_CONFIG.ignore_json_paths],
  };
}
export function mergeTenantProductConfig(
  config?: Partial<TenantProductConfig>,
): TenantProductConfig {
  return {
    ...createDefaultTenantProductConfig(),
    ...config,
    cbpro: mergeCBProPolicyConfig(config?.cbpro),
  };
}
type TenantProductApiItem = {
  id: string;
  product_id: string;
  product_code: string;
  product_name: string;
  description?: string;
  cb_pro_enabled?: boolean;
  config?: Partial<TenantProductConfig>;
  created_at: string;
  updated_at?: string;
};
function normalizeTenantProduct(product: TenantProductApiItem): TenantProduct {
  return {
    product_id: product.product_code,
    name: product.product_name,
    description: product.description,
    cb_pro_enabled: product.cb_pro_enabled,
    config: mergeTenantProductConfig(product.config),
    granted_at: product.created_at,
    updated_at: product.updated_at,
  };
}
export function fetchTenantList(
  params?: TenantListQuery,
  signal?: AbortSignal,
) {
  return request.get<TenantListResponse>({
    url: BASE_PATH,
    params,
    signal,
  });
}
export function fetchTenantById(code: string, signal?: AbortSignal) {
  return request.get<Tenant>({
    url: tenantPath(code),
    signal,
  });
}
export function createTenant(
  data: CreateTenantRequest,
  signal?: AbortSignal,
  options?: {
    showSuccessMessage?: boolean;
  },
) {
  return request.post<Tenant>({
    url: BASE_PATH,
    params: data,
    showSuccessMessage: options?.showSuccessMessage ?? true,
    signal,
  });
}
export function updateTenant(
  code: string,
  data: UpdateTenantRequest,
  signal?: AbortSignal,
) {
  return request.put<Tenant>({
    url: `${BASE_PATH}/${code}`,
    params: data,
    showSuccessMessage: true,
    signal,
  });
}
export function deleteTenant(code: string, signal?: AbortSignal) {
  return request.del<void>({
    url: `${BASE_PATH}/${code}`,
    showSuccessMessage: true,
    signal,
  });
}
export function setTenantEnabled(
  code: string,
  data: SetTenantEnabledRequest,
  signal?: AbortSignal,
) {
  return request.put<void>({
    url: `${BASE_PATH}/${code}/enabled`,
    params: data,
    showSuccessMessage: true,
    signal,
  });
}
export function fetchTenantCryptoProfile(code: string, signal?: AbortSignal) {
  return request.get<TenantCryptoProfile>({
    url: `${BASE_PATH}/${code}/crypto-profile`,
    signal,
  });
}
export function updateTenantCryptoProfile(
  code: string,
  data: UpdateTenantCryptoProfileRequest,
  signal?: AbortSignal,
) {
  return request.put<TenantCryptoProfile>({
    url: `${BASE_PATH}/${code}/crypto-profile`,
    params: data,
    showSuccessMessage: true,
    signal,
  });
}
export function fetchTenantCryptoKeys(code: string, signal?: AbortSignal) {
  return request.get<TenantCryptoKeyListResponse>({
    url: `${BASE_PATH}/${code}/crypto-keys`,
    signal,
  });
}
export function createTenantCryptoKey(code: string, signal?: AbortSignal) {
  return request.post<TenantCryptoKeyCreateResponse>({
    url: `${BASE_PATH}/${code}/crypto-keys`,
    showSuccessMessage: true,
    signal,
  });
}
export function rotateTenantCryptoKey(code: string, signal?: AbortSignal) {
  return request.post<TenantCryptoKeyCreateResponse>({
    url: `${BASE_PATH}/${code}/crypto-keys/rotate`,
    showSuccessMessage: true,
    signal,
  });
}
export function activateTenantCryptoKey(
  code: string,
  version: string,
  signal?: AbortSignal,
) {
  return request.post<TenantCryptoKey>({
    url: `${BASE_PATH}/${code}/crypto-keys/${version}/activate`,
    showSuccessMessage: true,
    signal,
  });
}
export function deprecateTenantCryptoKey(
  code: string,
  version: string,
  signal?: AbortSignal,
) {
  return request.post<TenantCryptoKey>({
    url: `${BASE_PATH}/${code}/crypto-keys/${version}/deprecate`,
    showSuccessMessage: true,
    signal,
  });
}
export function disableTenantCryptoKey(
  code: string,
  version: string,
  signal?: AbortSignal,
) {
  return request.post<TenantCryptoKey>({
    url: `${BASE_PATH}/${code}/crypto-keys/${version}/disable`,
    showSuccessMessage: true,
    signal,
  });
}
export function deleteTenantCryptoKey(
  code: string,
  version: string,
  signal?: AbortSignal,
) {
  return request.del<TenantCryptoKey>({
    url: `${BASE_PATH}/${code}/crypto-keys/${version}`,
    showSuccessMessage: true,
    signal,
  });
}
export function fetchTenantAccount(code: string, signal?: AbortSignal) {
  return request.get<TenantAccount>({
    url: `${BASE_PATH}/${code}/account`,
    signal,
  });
}
export function rechargeTenantAccount(
  code: string,
  data: RechargeRequest,
  signal?: AbortSignal,
) {
  return request.post<RechargeResponse>({
    url: `${BASE_PATH}/${code}/account/recharge`,
    params: data,
    showSuccessMessage: true,
    signal,
  });
}
export function refundTenantAccount(
  code: string,
  data: RefundRequest,
  signal?: AbortSignal,
) {
  return request.post<RefundResponse>({
    url: `${BASE_PATH}/${code}/account/refund`,
    params: data,
    showSuccessMessage: true,
    signal,
  });
}
export function getTenantTransactions(
  code: string,
  params?: TransactionListQuery,
  signal?: AbortSignal,
) {
  return request.get<TransactionListResponse>({
    url: `${BASE_PATH}/${code}/account/transactions`,
    params,
    signal,
  });
}
export function fetchTenantRateLimitConfig(
  tenantCode: string,
  signal?: AbortSignal,
) {
  return request.get<TenantRateLimitConfig>({
    url: `${BASE_PATH}/${tenantCode}/config/ratelimit`,
    signal,
  });
}
export function updateTenantRateLimitConfig(
  tenantCode: string,
  data: UpdateTenantRateLimitConfigRequest,
  signal?: AbortSignal,
) {
  return request.put<TenantRateLimitConfig>({
    url: `${BASE_PATH}/${tenantCode}/config/ratelimit`,
    params: data,
    showSuccessMessage: true,
    signal,
  });
}

export function fetchTenantRateLimitCommands(
  tenantCode: string,
  limit = 50,
  signal?: AbortSignal,
) {
  return request.get<RateLimitCommandListResponse>({
    url: `${BASE_PATH}/${tenantCode}/ratelimit/commands`,
    params: { limit },
    signal,
  });
}

export function createTenantRateLimitCommand(
  tenantCode: string,
  data: CreateRateLimitCommandRequest,
  signal?: AbortSignal,
) {
  return request.post<RateLimitControlCommand>({
    url: `${BASE_PATH}/${tenantCode}/ratelimit/commands`,
    params: data,
    showSuccessMessage: true,
    signal,
  });
}

export function cancelTenantRateLimitCommand(
  tenantCode: string,
  commandId: string,
  signal?: AbortSignal,
) {
  return request.post<RateLimitControlCommand>({
    url: `${BASE_PATH}/${tenantCode}/ratelimit/commands/${commandId}/cancel`,
    showSuccessMessage: true,
    signal,
  });
}
export function fetchTenantCacheConfig(
  tenantCode: string,
  signal?: AbortSignal,
) {
  return request.get<TenantCacheConfig>({
    url: `${BASE_PATH}/${tenantCode}/config/cache`,
    signal,
  });
}
export function updateTenantCacheConfig(
  tenantCode: string,
  data: UpdateTenantCacheConfigRequest,
  signal?: AbortSignal,
) {
  return request.put<TenantCacheConfig>({
    url: `${BASE_PATH}/${tenantCode}/config/cache`,
    params: data,
    showSuccessMessage: true,
    signal,
  });
}
export function fetchTenantApiKeys(tenantCode: string, signal?: AbortSignal) {
  return request.get<ApiKeyListResponse>({
    url: `${BASE_PATH}/${tenantCode}/api-keys`,
    signal,
  });
}
export function createApiKey(
  tenantCode: string,
  data: CreateApiKeyRequest,
  signal?: AbortSignal,
) {
  return request.post<CreateApiKeyResponse>({
    url: `${BASE_PATH}/${tenantCode}/api-keys`,
    params: data,
    showSuccessMessage: true,
    signal,
  });
}
export function deleteApiKey(
  tenantCode: string,
  keyId: string,
  signal?: AbortSignal,
) {
  return request.del<void>({
    url: `${BASE_PATH}/${tenantCode}/api-keys/${keyId}`,
    showSuccessMessage: true,
    signal,
  });
}
export function updateApiKeyWarmup(
  tenantCode: string,
  keyId: string,
  warmupEnabled: boolean,
  signal?: AbortSignal,
) {
  return request.request<{ key_prefix: string; warmup_enabled: boolean }>({
    url: `${BASE_PATH}/${tenantCode}/api-keys/${keyId}/warmup`,
    method: "PATCH",
    params: { warmup_enabled: warmupEnabled },
    showSuccessMessage: true,
    signal,
  });
}
export function fetchTenantProducts(tenantCode: string, signal?: AbortSignal) {
  return request
    .get<{
      tenant_code: string;
      products: TenantProductApiItem[];
      actions?: TenantProductListResponse["actions"];
    }>({
      url: tenantPath(tenantCode, "products"),
      signal,
    })
    .then(
      (res): TenantProductListResponse => ({
        tenant_id: res.tenant_code,
        products: res.products.map(normalizeTenantProduct),
        total: res.products.length,
        actions: res.actions,
      }),
    );
}
export function fetchTenantProductConfig(
  tenantCode: string,
  productId: string,
  signal?: AbortSignal,
) {
  return request
    .get<TenantProductApiItem>({
      url: tenantPath(tenantCode, "products", productId, "config"),
      signal,
    })
    .then(normalizeTenantProduct);
}
export function grantProduct(
  tenantCode: string,
  productId: string,
  config?: Partial<TenantProductConfig>,
  options?: {
    signal?: AbortSignal;
    showSuccessMessage?: boolean;
  },
) {
  const params = config
    ? {
        tenant_code: tenantCode,
        config: {
          ...mergeTenantProductConfig(config),
          currency: "CNY",
        },
      }
    : { tenant_code: tenantCode };
  return request.post<void>({
    url: `${PRODUCT_BASE_PATH}/${productId}/grant`,
    params,
    showSuccessMessage: options?.showSuccessMessage ?? true,
    signal: options?.signal,
  });
}
export function revokeProduct(
  tenantCode: string,
  productId: string,
  signal?: AbortSignal,
) {
  return request.post<void>({
    url: `${PRODUCT_BASE_PATH}/${productId}/revoke`,
    params: { tenant_code: tenantCode },
    showSuccessMessage: true,
    signal,
  });
}
export function updateTenantProductConfig(
  tenantCode: string,
  productId: string,
  data: UpdateTenantProductConfigRequest,
  signal?: AbortSignal,
) {
  return request.put<void>({
    url: tenantPath(tenantCode, "products", productId, "config"),
    params: data,
    showSuccessMessage: true,
    signal,
  });
}
export function fetchTenantWhitelist(tenantCode: string, signal?: AbortSignal) {
  return request.get<WhitelistListResponse>({
    url: `${BASE_PATH}/${tenantCode}/whitelist`,
    signal,
  });
}
export function addToWhitelist(
  tenantCode: string,
  data: AddToWhitelistRequest,
  signal?: AbortSignal,
) {
  return request.post<AddToWhitelistResponse>({
    url: `${BASE_PATH}/${tenantCode}/whitelist`,
    params: data,
    showSuccessMessage: true,
    signal,
  });
}
export function removeFromWhitelist(
  tenantCode: string,
  ipAddress: string,
  signal?: AbortSignal,
) {
  return request.del<RemoveFromWhitelistResponse>({
    url: `${BASE_PATH}/${tenantCode}/whitelist/${encodeURIComponent(ipAddress)}`,
    showSuccessMessage: true,
    signal,
  });
}
