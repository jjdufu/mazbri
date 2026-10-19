import request from "@/utils/http";
import type {
  PlatformStats,
  UsageAnalytics,
  AnalyticsQuery,
} from "@/types/api/analytics";
import type { RequestOptions } from "@/types/api/common";
import { API_BASE_PATH } from "@/config/api";
export function fetchPlatformStats(
  params?: AnalyticsQuery,
  options?: RequestOptions,
) {
  return request.get<PlatformStats>({
    url: `${API_BASE_PATH}/stats/platform`,
    params,
    signal: options?.signal,
  });
}
export function fetchUsageAnalytics(
  tenantCode: string,
  params?: AnalyticsQuery,
  options?: RequestOptions,
) {
  return request.get<UsageAnalytics>({
    url: `${API_BASE_PATH}/tenants/${tenantCode}/analytics`,
    params,
    signal: options?.signal,
  });
}
