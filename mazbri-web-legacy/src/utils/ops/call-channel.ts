/**
 * 调用日志「通道」列的显示语义（纯逻辑，便于单测）。
 *
 * - 有通道：显示通道名（回退通道编码）；缓存命中 / CBPro 兜底加 `CB:` 前缀。
 * - 没有通道但缓存命中 / 兜底：显示 `CB`。
 * - 没有通道：请求在路由到通道前就结束了（网关拒绝：鉴权失败、产品不存在、限流、余额不足等），
 *   显示「未路由」，有 product_id 时附带产品，悬停显示 error_code。
 */
export interface CallChannelRow {
  channel_name?: string | null;
  channel_code?: string | null;
  error_code?: string | null;
  detail?: Record<string, unknown> | null;
}

export type CallChannelView =
  | { kind: "channel"; text: string }
  | { kind: "unrouted"; productId: string; errorCode: string };

const str = (value: unknown): string =>
  typeof value === "string" ? value.trim() : "";

export function resolveCallChannel(
  row: CallChannelRow | null | undefined,
): CallChannelView {
  const detail = (row?.detail ?? {}) as Record<string, unknown>;
  const cb = detail.cache_hit === true || detail.fallback_used === true;
  const raw = str(row?.channel_name) || str(row?.channel_code);
  if (raw) {
    const prefixed =
      raw === "CB" || raw.startsWith("CB:") || raw.startsWith("CB/");
    return { kind: "channel", text: cb && !prefixed ? `CB:${raw}` : raw };
  }
  if (cb) return { kind: "channel", text: "CB" };
  return {
    kind: "unrouted",
    productId: str(detail.product_id),
    errorCode: str(row?.error_code) || str(detail.error_code),
  };
}
