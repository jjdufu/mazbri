import type { RouteLocationRaw } from "vue-router";
import { RUNTIME_PATH, buildRuntimeRoute } from "./runtime-link";

/**
 * 服务列表已并入运行指标；常量指向 RUNTIME_PATH，供详情 activePath 与旧调用方对齐。
 * 服务详情仍挂隐藏深链 `/ops/services/:service`（侧栏无「服务概览」）。
 */
export const SERVICES_PATH = RUNTIME_PATH;

/** @deprecated 仅兼容旧「health」深链；一律落到运行指标 apps。 */
export type ServicesTabId = "health" | "apps";

export interface ServicesLinkOptions {
  tab?: ServicesTabId;
}

/**
 * 构建运行指标·应用组件路由。
 * - 默认 / 显式 health|apps → ?tab=apps
 */
export function buildServicesRoute(
  options: ServicesLinkOptions = {},
): RouteLocationRaw {
  void options.tab;
  return buildRuntimeRoute({ tab: "apps" });
}

/** 服务详情（仅超管路由；非超管勿调用）。 */
export function buildServiceDetailRoute(service: string): RouteLocationRaw {
  return { path: `/ops/services/${encodeURIComponent(service)}` };
}
