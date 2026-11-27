/**
 * 上游「认证失效识别规则」（`auth_flow.invalidation` / `egress.auth.invalidation`）的表单模型。
 *
 * 与后端 `mazbri_core::AuthInvalidationSpec` 一一对应：
 * - `http_status`：命中即视为令牌失效的 HTTP 状态码（400..=599，默认 [401]）；
 * - `jsonrpc_codes`：JSON-RPC `error.code`（仅 jsonrpc 适配器）；
 * - `body_match`：响应体 JSONPath 取值按 JSON 值严格相等匹配；
 * - `ws_close_before_reply`：WS 新连接回复前被关闭视为失效（仅 ws_unary 适配器）。
 *
 * 表单 `enabled=false` 表示不显式声明，输出时省略整个字段（运行时回退到上层规则 / 默认 401）。
 */
export interface BodyMatchRow {
  path: string;
  /** 逗号分隔的 JSON 字面量，例如 `"TOKEN_EXPIRED", 40101`；非 JSON 文本按字符串处理。 */
  equals: string;
}

export interface InvalidationForm {
  enabled: boolean;
  http_status: number[];
  jsonrpc_codes: number[];
  body_match: BodyMatchRow[];
  ws_close_before_reply: boolean;
}

export type InvalidationSource = "channel" | "supplier" | "default";

export const DEFAULT_REJECT_STATUS = 401;

export function createEmptyInvalidation(): InvalidationForm {
  return {
    enabled: false,
    http_status: [DEFAULT_REJECT_STATUS],
    jsonrpc_codes: [],
    body_match: [],
    ws_close_before_reply: false,
  };
}

function numberList(raw: unknown): number[] {
  if (!Array.isArray(raw)) return [];
  return raw.map(Number).filter((n) => Number.isInteger(n));
}

function equalsToText(values: unknown): string {
  if (!Array.isArray(values)) return "";
  return values.map((v) => JSON.stringify(v)).join(", ");
}

/** 把逗号分隔的文本解析为 JSON 值列表：能按 JSON 解析的取 JSON 值，否则按字符串。 */
export function parseEqualsText(text: string): unknown[] {
  return text
    .split(",")
    .map((part) => part.trim())
    .filter((part) => part !== "")
    .map((part) => {
      try {
        return JSON.parse(part) as unknown;
      } catch {
        return part;
      }
    });
}

export function invalidationFromApi(raw: unknown): InvalidationForm {
  if (!raw || typeof raw !== "object") return createEmptyInvalidation();
  const obj = raw as Record<string, unknown>;
  const rows = Array.isArray(obj.body_match) ? obj.body_match : [];
  return {
    enabled: true,
    http_status:
      "http_status" in obj
        ? numberList(obj.http_status)
        : [DEFAULT_REJECT_STATUS],
    jsonrpc_codes: numberList(obj.jsonrpc_codes),
    body_match: rows.map((r) => {
      const row = (r || {}) as Record<string, unknown>;
      return { path: String(row.path || ""), equals: equalsToText(row.equals) };
    }),
    ws_close_before_reply: Boolean(obj.ws_close_before_reply),
  };
}

/** 输出给 API；未启用时返回 `undefined`（调用方据此省略字段，不写 `{}`）。 */
export function invalidationToApi(
  form: InvalidationForm | undefined,
): Record<string, unknown> | undefined {
  if (!form?.enabled) return undefined;
  const out: Record<string, unknown> = { http_status: [...form.http_status] };
  if (form.jsonrpc_codes.length) out.jsonrpc_codes = [...form.jsonrpc_codes];
  const rows = form.body_match
    .filter((r) => r.path.trim() !== "")
    .map((r) => ({ path: r.path.trim(), equals: parseEqualsText(r.equals) }));
  if (rows.length) out.body_match = rows;
  if (form.ws_close_before_reply) out.ws_close_before_reply = true;
  return out;
}

/** 字段可见性：JSON-RPC 错误码只对 jsonrpc 适配器，WS 关闭开关只对 ws_unary；未知适配器（供应商级）全部显示。 */
export function invalidationFields(adapter: string): {
  jsonrpc: boolean;
  wsClose: boolean;
} {
  const a = String(adapter || "").trim();
  return {
    jsonrpc: a === "" || a === "jsonrpc",
    wsClose: a === "" || a === "ws_unary",
  };
}

export type InvalidationIssue =
  | { kind: "status_range"; value: number }
  | { kind: "status_duplicate"; value: number }
  | { kind: "jsonrpc_duplicate"; value: number }
  | { kind: "body_path"; index: number }
  | { kind: "body_equals"; index: number };

/** 与后端 `validate_invalidation_rules` 一致的前端校验；返回第一个问题，无问题返回 `null`。 */
export function validateInvalidation(
  form: InvalidationForm,
): InvalidationIssue | null {
  if (!form.enabled) return null;
  const seen = new Set<number>();
  for (const code of form.http_status) {
    if (!Number.isInteger(code) || code < 400 || code > 599) {
      return { kind: "status_range", value: code };
    }
    if (seen.has(code)) return { kind: "status_duplicate", value: code };
    seen.add(code);
  }
  const seenRpc = new Set<number>();
  for (const code of form.jsonrpc_codes) {
    if (seenRpc.has(code)) return { kind: "jsonrpc_duplicate", value: code };
    seenRpc.add(code);
  }
  for (const [index, row] of form.body_match.entries()) {
    const path = row.path.trim();
    if (path === "" && row.equals.trim() === "") continue;
    if (!path.startsWith("$") || path === "$")
      return { kind: "body_path", index };
    if (parseEqualsText(row.equals).length === 0)
      return { kind: "body_equals", index };
  }
  return null;
}

/** 规则来源：渠道 `egress.auth.invalidation` 优先，其次已启用的供应商 `auth_flow.invalidation`，否则默认。 */
export function invalidationSource(
  channelAuth: Record<string, unknown> | null | undefined,
  supplierFlow:
    | { enabled?: boolean; invalidation?: InvalidationForm }
    | null
    | undefined,
): InvalidationSource {
  if (
    channelAuth &&
    channelAuth.invalidation &&
    typeof channelAuth.invalidation === "object"
  ) {
    return "channel";
  }
  if (supplierFlow?.enabled && supplierFlow.invalidation?.enabled)
    return "supplier";
  return "default";
}

export interface RetryInfo {
  count: number;
  reasons: string[];
}

/**
 * 从试运行阶段的 `response_summary` 中汇总引擎内部重试（`internal_retries` / `retry_reasons`），
 * 例如 `auth_rejected:jsonrpc_code`、`stale_connection`。没有重试时返回 `null`。
 */
export function extractRetryInfo(
  stages: { response_summary?: Record<string, unknown> }[] | null | undefined,
): RetryInfo | null {
  let count = 0;
  const reasons: string[] = [];
  for (const stage of stages || []) {
    const rs = stage.response_summary;
    if (!rs) continue;
    const n = Number(rs.internal_retries);
    if (Number.isInteger(n) && n > 0) count += n;
    if (Array.isArray(rs.retry_reasons)) {
      for (const r of rs.retry_reasons) reasons.push(String(r));
    }
  }
  return count > 0 ? { count, reasons } : null;
}
