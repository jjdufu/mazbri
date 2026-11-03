import type { RouteLocationRaw } from "vue-router";

/** 可靠性（容量规划）父路由；仅超管。 */
export const RELIABILITY_PATH = "/ops/reliability";

/**
 * 构建可靠性路由。
 * 当前无 Tab；预留 query 扩展位，避免调用方散落魔法字符串。
 */
export function buildReliabilityRoute(): RouteLocationRaw {
  return { path: RELIABILITY_PATH };
}
