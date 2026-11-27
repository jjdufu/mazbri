export interface BodyMatchRow {
  path: string;
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
      return {
        path: String(row.path || ""),
        equals: equalsToText(row.equals),
      };
    }),
    ws_close_before_reply: Boolean(obj.ws_close_before_reply),
  };
}

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
    if (!path.startsWith("$") || path === "$") {
      return { kind: "body_path", index };
    }
    if (parseEqualsText(row.equals).length === 0) {
      return { kind: "body_equals", index };
    }
  }
  return null;
}

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
  if (supplierFlow?.enabled && supplierFlow.invalidation?.enabled) {
    return "supplier";
  }
  return "default";
}
