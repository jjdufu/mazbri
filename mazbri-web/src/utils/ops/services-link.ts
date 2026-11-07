import type { RouteLocationRaw } from "vue-router";

/** 服务详情（仅超管路由；非超管勿调用）。 */
export function buildServiceDetailRoute(service: string): RouteLocationRaw {
  return { path: `/ops/services/${encodeURIComponent(service)}` };
}
