import request from "@/utils/http";
import type {
  PlatformInventory,
  UsageAnalytics,
  AnalyticsQuery,
} from "@/types/api/analytics";
import type { RequestOptions } from "@/types/api/common";
import { API_BASE_PATH } from "@/config/api";
/** 租户 / 产品规模（非时间窗）。 */
export function fetchPlatformInventory(options?: RequestOptions) {
  return request.get<PlatformInventory>({
    url: `${API_BASE_PATH}/stats/ops/inventory`,
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
