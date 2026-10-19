import type {
  TenantCacheConfig,
  TenantProductConfig,
  TenantRateLimitConfig,
  UpdateTenantCacheConfigRequest,
  UpdateTenantRateLimitConfigRequest,
} from "@/types/api/tenant";
import { $t } from "@/locales";
type ConflictField = {
  labelKey: string;
  attempted: unknown;
  latest: unknown;
};
function stringifyValue(value: unknown): string {
  if (typeof value === "boolean") {
    return value ? $t("common.status.enabled") : $t("common.status.disabled");
  }
  if (value == null) return "-";
  if (Array.isArray(value)) return value.length ? value.join(", ") : "-";
  if (typeof value === "object") return JSON.stringify(value, null, 2);
  return String(value);
}
function isSameValue(left: unknown, right: unknown): boolean {
  return stringifyValue(left) === stringifyValue(right);
}
function buildConflictSummary(
  scopeKey: string,
  updatedAt: string | undefined,
  fields: ConflictField[],
): string {
  const changedFields = fields.filter(
    (field) => !isSameValue(field.attempted, field.latest),
  );
  const scopeLabel = $t(scopeKey);
  if (changedFields.length === 0) {
    return [
      $t("common.conflict.updatedByOthers", { scopeLabel }),
      $t("common.conflict.latestUpdatedAt", { updatedAt: updatedAt ?? "-" }),
    ].join("\n");
  }
  return [
    $t("common.conflict.recheckLatest", { scopeLabel }),
    $t("common.conflict.latestUpdatedAt", { updatedAt: updatedAt ?? "-" }),
    "",
    ...changedFields.flatMap((field) => [
      $t(field.labelKey),
      $t("common.conflict.yourSubmission", {
        value: stringifyValue(field.attempted),
      }),
      $t("common.conflict.latestValue", {
        value: stringifyValue(field.latest),
      }),
      "",
    ]),
  ].join("\n");
}
export function summarizeRateLimitConflict(
  attempted: UpdateTenantRateLimitConfigRequest,
  latest: TenantRateLimitConfig,
): string {
  return buildConflictSummary(
    "common.conflict.rateLimitScope",
    latest.updated_at,
    [
      {
        labelKey: "common.conflict.rateLimitEnabled",
        attempted: attempted.enabled,
        latest: latest.enabled,
      },
      {
        labelKey: "common.conflict.rateLimitMaxConcurrent",
        attempted: attempted.max_concurrent,
        latest: latest.max_concurrent,
      },
      {
        labelKey: "common.conflict.rateLimitBurstSize",
        attempted: attempted.burst_size,
        latest: latest.burst_size,
      },
      {
        labelKey: "common.conflict.rateLimitClientIpQps",
        attempted: attempted.client_ip_qps,
        latest: latest.client_ip_qps,
      },
      {
        labelKey: "common.conflict.rateLimitClientIpBurst",
        attempted: attempted.client_ip_burst,
        latest: latest.client_ip_burst,
      },
      {
        labelKey: "common.conflict.rateLimitMaxBandwidthMbps",
        attempted: attempted.max_bandwidth_mbps,
        latest: latest.max_bandwidth_mbps,
      },
      {
        labelKey: "common.conflict.rateLimitApiOverride",
        attempted: attempted.api_limits,
        latest: latest.api_limits,
      },
    ],
  );
}
export function summarizeCacheConflict(
  attempted: UpdateTenantCacheConfigRequest,
  latest: TenantCacheConfig,
): string {
  return buildConflictSummary("common.conflict.cacheScope", latest.updated_at, [
    {
      labelKey: "common.conflict.cacheDefaultTtlSecs",
      attempted: attempted.default_ttl,
      latest: latest.default_ttl,
    },
    {
      labelKey: "common.conflict.cacheMaxCacheSizeBytes",
      attempted: attempted.max_cache_size,
      latest: latest.max_cache_size,
    },
    {
      labelKey: "common.conflict.cacheTenantIsolated",
      attempted: attempted.tenant_isolated,
      latest: latest.tenant_isolated,
    },
    {
      labelKey: "common.conflict.cacheWarmupEnabled",
      attempted: attempted.warmup_enabled,
      latest: latest.warmup_enabled,
    },
    {
      labelKey: "common.conflict.cacheApiTtlOverride",
      attempted: attempted.api_ttl,
      latest: latest.api_ttl,
    },
    {
      labelKey: "common.conflict.cacheWarmupKeys",
      attempted: attempted.warmup_keys,
      latest: latest.warmup_keys,
    },
  ]);
}
export function summarizeTenantProductConflict(
  attempted: TenantProductConfig,
  latest: TenantProductConfig,
  updatedAt?: string,
): string {
  return buildConflictSummary("common.conflict.productScope", updatedAt, [
    {
      labelKey: "common.conflict.productBillingMode",
      attempted: attempted.billing_type,
      latest: latest.billing_type,
    },
    {
      labelKey: "common.conflict.productUnitPrice",
      attempted: attempted.unit_price,
      latest: latest.unit_price,
    },
    {
      labelKey: "common.conflict.productCbProFallback",
      attempted: attempted.cb_pro_fallback,
      latest: latest.cb_pro_fallback,
    },
    {
      labelKey: "common.conflict.productCodecEnabled",
      attempted: attempted.codec_enabled,
      latest: latest.codec_enabled,
    },
    {
      labelKey: "common.conflict.productRequestCodec",
      attempted: attempted.request_codec,
      latest: latest.request_codec,
    },
    {
      labelKey: "common.conflict.productResponseCodec",
      attempted: attempted.response_codec,
      latest: latest.response_codec,
    },
    {
      labelKey: "common.conflict.productCodecVersion",
      attempted: attempted.codec_version,
      latest: latest.codec_version,
    },
    {
      labelKey: "common.conflict.productCbProPolicy",
      attempted: attempted.cbpro,
      latest: latest.cbpro,
    },
  ]);
}
