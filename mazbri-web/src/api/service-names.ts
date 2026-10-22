import request from "@/utils/http";
import { API_BASE_PATH, API_PATHS } from "@/config/api";

export interface CanonicalServiceNames {
  services: string[];
  global: string;
}

let cache: Promise<CanonicalServiceNames> | undefined;

/** 规范服务名 + global，来源 mgmt（mazbri-core 单一真源），会话内缓存 */
export function fetchCanonicalServiceNames(): Promise<CanonicalServiceNames> {
  cache ??= request
    .get<CanonicalServiceNames>({
      url: `${API_BASE_PATH}${API_PATHS.CANONICAL_SERVICE_NAMES}`,
    })
    .catch((error: unknown) => {
      cache = undefined;
      throw error;
    });
  return cache;
}
