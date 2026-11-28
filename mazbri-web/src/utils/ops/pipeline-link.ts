import type { RouteLocationRaw } from "@/types/router";

/** 数据管道监控页（计费管道 + 控制面热更新 + 消息与存储时序）。 */
export const PIPELINE_PATH = "/ops/pipeline";

export type PipelineTabId = "billing" | "control" | "storage";

export interface PipelineLinkOptions {
  tab?: PipelineTabId;
}

export function buildPipelineRoute(
  options: PipelineLinkOptions = {},
): RouteLocationRaw {
  const query: Record<string, string> = {};
  if (options.tab) query.tab = options.tab;
  return { path: PIPELINE_PATH, query };
}
