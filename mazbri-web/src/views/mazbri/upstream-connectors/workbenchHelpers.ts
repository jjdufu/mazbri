

import type {
  AssertionSpec,
  ConnectorEndpoint,
  ConnectorRun,
  DraftGateItem,
  StageResult,
} from "@/api/upstream-connectors";

/** Hoppscotch Header List row. Edit-time truth is this array (order, dup keys, enabled, desc). Empty table is []. */
export type KvRow = {
  key: string;
  value: string;
  desc: string;
  enabled: boolean;
  /** Client Sortable key. Engine / maps ignore it. */
  uid?: string;
};

function newKvUid(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `kv-${Date.now().toString(36)}-${Math.random().toString(16).slice(2)}`;
}

export const WRITE_METHODS = new Set(["POST", "PUT", "PATCH", "DELETE"]);
export const HTTP_METHOD_PRESETS = [
  "GET",
  "POST",
  "PUT",
  "PATCH",
  "DELETE",
  "HEAD",
  "OPTIONS",
  "CONNECT",
  "TRACE",
] as const;

export const HTTP_METHODS = [...HTTP_METHOD_PRESETS, "CUSTOM"] as const;
export const EGRESS_ADAPTERS = [
  "http_json",
  "http_envelope",
  "sign_form",
  "jsonrpc",
  "graphql",
  "grpc",
  "soap_xml",
  "ws_unary",
  "job_poll",
] as const;
export {
  AUTH_PICKER_IDS,
  AUTH_PROFILES,
  authFieldGroup,
  authPickerFromState,
  authProfileFromPicker,
  sanitizeAuthSpec,
  visibleAuthPickers,
} from "./authMatrix";
export type { AuthFieldGroup, AuthPickerId, AuthProfileId } from "./authMatrix";
export const WRAP_OPTIONS = ["method_biz_content", "none"] as const;
export const HSPLIT_DEFAULT_LEFT = 240;
export const VSPLIT_DEFAULT_BOTTOM = 280;
export const VSPLIT_COLLAPSED_HEIGHT = 0;

export type HSplitState = { left: number; collapsed: boolean };
export type VSplitState = { bottom: number; collapsed: boolean };

function finiteNonNeg(n: unknown, fallback: number): number {
  return typeof n === "number" && Number.isFinite(n) && n >= 0 ? n : fallback;
}

function asPersistObject(raw: string): Record<string, unknown> | null {
  try {
    const p: unknown = JSON.parse(raw);
    if (p == null || typeof p !== "object" || Array.isArray(p)) return null;
    return p as Record<string, unknown>;
  } catch {
    return null;
  }
}


export function parseHSplitState(raw: string | null): HSplitState {
  if (!raw) return { left: HSPLIT_DEFAULT_LEFT, collapsed: true };
  const p = asPersistObject(raw);
  if (!p) return { left: HSPLIT_DEFAULT_LEFT, collapsed: true };
  return {
    left: finiteNonNeg(p.left, HSPLIT_DEFAULT_LEFT),
    collapsed: p.collapsed !== false,
  };
}


export function parseVSplitState(raw: string | null): VSplitState {
  if (!raw) return { bottom: VSPLIT_DEFAULT_BOTTOM, collapsed: true };
  const p = asPersistObject(raw);
  if (!p) return { bottom: VSPLIT_DEFAULT_BOTTOM, collapsed: true };
  return {
    bottom: finiteNonNeg(p.bottom, VSPLIT_DEFAULT_BOTTOM),
    collapsed: p.collapsed !== false,
  };
}


export const CODE_FREE_UPSTREAM_TYPE = "transparent";

export function isMicroserviceUpstream(d: {
  upstream_type?: string | null;
  config?: Record<string, unknown> | null;
}): boolean {
  if (d.upstream_type === "microservice") return true;
  return d.config?.upstream_type === "microservice";
}

export function asCodeFreeDraft<
  T extends { upstream_type: string; config?: Record<string, unknown> | null },
>(d: T): T {
  const config: Record<string, unknown> = { ...(d.config || {}) };
  config.upstream_type = CODE_FREE_UPSTREAM_TYPE;
  delete config.contract_id;
  return { ...d, upstream_type: CODE_FREE_UPSTREAM_TYPE, config };
}

export const ASSERT_TYPES = [
  "status_range",
  "status_in",
  "latency_ms_lte",
  "jsonpath_exists",
  "jsonpath_equals",
  "jsonpath_matches",
  "body_contains",
  "body_not_contains",
  "header_equals",
  "header_exists",
] as const;

export const STAGE_ORDER = [
  "validate",
  "ssrf",
  "auth",
  "request_transform",
  "upstream",
] as const;

export function emptyKvRow(): KvRow {
  return { key: "", value: "", desc: "", enabled: true, uid: newKvUid() };
}

export function normalizeKvRow(
  row: Partial<KvRow> & { key?: string; value?: string },
): KvRow {
  const uid = row.uid || newKvUid();
  if (!row.uid) {
    row.uid = uid;
  }
  return {
    key: row.key ?? "",
    value: row.value ?? "",
    desc: row.desc ?? "",
    enabled: row.enabled !== false,
    uid,
  };
}

export function isKvBlank(row: KvRow): boolean {
  return !row.key.trim() && !row.value.trim() && !row.desc.trim();
}

/**
 * Upgrade a map to rows. Empty map → one blank row (Hopp: Headers opens with one empty input).
 * Filled maps are 1:1 — do not append a trailing blank here. Interaction append lives in WorkbenchKvRows.
 */
export function mapToKv(obj?: Record<string, string> | null): KvRow[] {
  const rows = Object.entries(obj || {}).map(([key, value]) =>
    normalizeKvRow({ key, value, desc: "", enabled: true }),
  );
  return rows.length ? rows : [emptyKvRow()];
}

/** Drop stacked trailing blanks; never insert a blank. Empty stays []. */
export function collapseTrailingBlanks(rows: KvRow[]): KvRow[] {
  const next = rows.map(normalizeKvRow);
  while (
    next.length > 1 &&
    isKvBlank(next[next.length - 1]!) &&
    isKvBlank(next[next.length - 2]!)
  ) {
    next.pop();
  }
  return next;
}

/** Prefer stored entries (including []). Only fall back to map when entries is absent. */
export function entriesToKv(
  entries?: Array<Partial<KvRow>> | null,
  fallback?: Record<string, string> | null,
): KvRow[] {
  if (Array.isArray(entries)) {
    return collapseTrailingBlanks(entries.map(normalizeKvRow));
  }
  return mapToKv(fallback);
}

export function kvToEntries(rows: KvRow[]): KvRow[] {
  return rows.map(normalizeKvRow);
}

/** Load workbench_env: array of KvRow (incl. []) or legacy string map. */
export function workbenchEnvToKv(value: unknown): KvRow[] {
  if (Array.isArray(value)) {
    return entriesToKv(value as Array<Partial<KvRow>>);
  }
  if (value && typeof value === "object") {
    const rec: Record<string, string> = {};
    for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
      if (typeof v === "string") rec[k] = v;
      else if (typeof v === "number" || typeof v === "boolean") rec[k] = String(v);
    }
    return mapToKv(rec);
  }
  return mapToKv({});
}

/** Egress / engine map only. Last duplicate key wins. Disabled and blank keys dropped. */
export function kvToMap(
  rows: Array<{ key?: string; value?: string; enabled?: boolean }>,
) {
  const out: Record<string, string> = {};
  for (const row of rows) {
    if (row.enabled === false) continue;
    const k = row.key?.trim();
    if (k) out[k] = row.value ?? "";
  }
  return out;
}

export function kvEnabledCount(rows: KvRow[]): number {
  return rows.filter((r) => r.enabled !== false && r.key.trim()).length;
}

export function duplicateKvKeys(rows: KvRow[]): string[] {
  const counts = new Map<string, number>();
  for (const row of rows) {
    const k = row.key.trim();
    if (!k) continue;
    counts.set(k, (counts.get(k) || 0) + 1);
  }
  return [...counts.entries()].filter(([, n]) => n > 1).map(([k]) => k);
}

export function stripEmptyKv<T extends { key?: string; value?: string; desc?: string }>(
  rows: T[],
): T[] {
  return rows.filter(
    (r) =>
      Boolean(r.key?.trim()) ||
      Boolean(r.value?.trim()) ||
      Boolean(r.desc?.trim()),
  );
}

export function serializeKvBulk(rows: KvRow[]): string {
  return stripEmptyKv(rows)
    .map((r) => {
      const core = r.desc.trim()
        ? `${r.key}: ${r.value} # ${r.desc}`
        : `${r.key}: ${r.value}`;
      return r.enabled === false ? `# ${core}` : core;
    })
    .join("\n");
}

export function parseKvBulk(text: string): KvRow[] {
  const parsed: KvRow[] = [];
  for (const line of text.split("\n")) {
    if (!line.trim()) continue;
    let enabled = true;
    let t = line.trim();
    if (t.startsWith("# ")) {
      enabled = false;
      t = t.slice(2);
    }
    const colon = t.indexOf(":");
    let key = "";
    let rest = "";
    if (colon >= 0) {
      key = t.slice(0, colon).trim();
      rest = t.slice(colon + 1);
      if (rest.startsWith(" ")) rest = rest.slice(1);
    } else {
      key = t.trim();
    }
    let value = rest;
    let desc = "";
    const hash = rest.indexOf(" # ");
    if (hash >= 0) {
      value = rest.slice(0, hash);
      desc = rest.slice(hash + 3);
    }
    parsed.push(normalizeKvRow({ key, value, desc, enabled }));
  }
  return parsed;
}

/** Merge URL search into existing query rows, keeping desc/enabled for matching keys. */
export function mergeQueryFromMap(
  query: Record<string, string>,
  prev: KvRow[],
): KvRow[] {
  const next: KvRow[] = [];
  const used = new Set<number>();
  for (const [k, v] of Object.entries(query)) {
    const idx = prev.findIndex((r, i) => !used.has(i) && r.key === k);
    if (idx >= 0) {
      used.add(idx);
      const old = prev[idx]!;
      next.push(
        normalizeKvRow({
          key: k,
          value: v,
          desc: old.desc,
          enabled: old.enabled,
        }),
      );
    } else {
      next.push(normalizeKvRow({ key: k, value: v }));
    }
  }
  for (let i = 0; i < prev.length; i++) {
    if (used.has(i)) continue;
    const r = prev[i]!;
    if ((r.enabled === false && r.key.trim()) || isKvBlank(r)) {
      next.push(normalizeKvRow(r));
    }
  }
  return collapseTrailingBlanks(next);
}

const METHOD_TOKEN_RE = /^[A-Z0-9+-]{1,16}$/;

export function isLegalHttpMethod(raw: string): boolean {
  const t = (raw || "").trim().toUpperCase();
  if (!t || t === "CUSTOM") return false;
  if (t.includes("://") || /[\s/?#]/.test(t)) return false;
  return METHOD_TOKEN_RE.test(t);
}

export function sanitizeHttpMethod(raw: string): string | null {
  const t = (raw || "").trim().toUpperCase();
  return isLegalHttpMethod(t) ? t : null;
}

export function displayMethodBadge(raw?: string): string {
  const t = (raw || "GET").trim().toUpperCase();
  if (isLegalHttpMethod(t)) return t;
  if (!t) return "GET";
  return t.slice(0, 8);
}

/** L1 tenant body: none = no business body; json = tenant JSON. Never a MIME. */
export type BodyContentType = "none" | "json";

export type BodyEncoding = "json" | "form" | "none";

export type EnvTextSegment =
  | { kind: "text"; text: string }
  | { kind: "var"; raw: string; name: string; found: boolean };

const INCOMPLETE_ENV_RE = /\{\{\s*([\w.-]*)$/;


export function incompleteEnvVar(
  text: string,
  caret: number,
): { start: number; prefix: string } | null {
  if (caret < 0 || caret > text.length) return null;
  const before = text.slice(0, caret);
  const m = INCOMPLETE_ENV_RE.exec(before);
  if (!m || m.index == null) return null;
  return { start: m.index, prefix: m[1] };
}

export function filterEnvKeys(
  env: Record<string, string>,
  prefix: string,
): string[] {
  const p = (prefix || "").toLowerCase();
  return Object.keys(env)
    .filter((k) => k && (!p || k.toLowerCase().startsWith(p)))
    .sort((a, b) => a.localeCompare(b));
}

export function applyEnvVarCompletion(
  text: string,
  caret: number,
  name: string,
): { text: string; caret: number } | null {
  const hit = incompleteEnvVar(text, caret);
  if (!hit || !name) return null;
  const insert = `{{${name}}}`;
  const next = `${text.slice(0, hit.start)}${insert}${text.slice(caret)}`;
  return { text: next, caret: hit.start + insert.length };
}


export function splitEnvText(
  input: string,
  env: Record<string, string>,
): EnvTextSegment[] {
  const re = /\{\{\s*([\w.-]+)\s*\}\}/g;
  const out: EnvTextSegment[] = [];
  let last = 0;
  let m: RegExpExecArray | null = re.exec(input);
  while (m) {
    if (m.index > last) {
      out.push({ kind: "text", text: input.slice(last, m.index) });
    }
    out.push({
      kind: "var",
      raw: m[0],
      name: m[1],
      found: Object.prototype.hasOwnProperty.call(env, m[1]),
    });
    last = m.index + m[0].length;
    m = re.exec(input);
  }
  if (last < input.length) {
    out.push({ kind: "text", text: input.slice(last) });
  }
  return out;
}

export function composeRequestUrl(
  base: string,
  path: string,
  query?: Record<string, string> | KvRow[],
): string {
  const b = (base || "").trim().replace(/\/+$/, "");
  let p = (path || "").trim();
  const qcut = p.indexOf("?");
  if (qcut >= 0) p = p.slice(0, qcut);
  if (p && !p.startsWith("/") && !/^https?:\/\//i.test(p)) p = `/${p}`;

  let joined: string;
  if (/^https?:\/\//i.test(p)) joined = p.replace(/\/+$/, "") || p;
  else if (!b) joined = p;
  else if (!p || p === "/") joined = b;
  else joined = `${b}${p.startsWith("/") ? p : `/${p}`}`;

  const qmap = Array.isArray(query) ? kvToMap(query) : query || {};
  const parts: string[] = [];
  for (const [k, v] of Object.entries(qmap)) {
    if (!k) continue;
    parts.push(`${encodeURIComponent(k)}=${encodeURIComponent(v ?? "")}`);
  }
  return parts.length ? `${joined}?${parts.join("&")}` : joined;
}

export function trySplitRequestUrl(
  url: string,
): { base: string; path: string; query: Record<string, string> } | null {
  const raw = (url || "").trim();
  if (!raw) return { base: "", path: "", query: {} };
  const query: Record<string, string> = {};
  if (/^https?:\/\//i.test(raw)) {
    try {
      const u = new URL(raw);
      if (!u.host) return null;
      u.searchParams.forEach((v, k) => {
        query[k] = v;
      });
      return { base: u.origin, path: u.pathname || "/", query };
    } catch {
      return null;
    }
  }
  const qi = raw.indexOf("?");
  const pathPart = (qi >= 0 ? raw.slice(0, qi) : raw) || "/";
  if (qi >= 0) {
    new URLSearchParams(raw.slice(qi + 1)).forEach((v, k) => {
      query[k] = v;
    });
  }
  return { base: "", path: pathPart, query };
}

export function sameStringMap(
  a?: Record<string, string> | null,
  b?: Record<string, string> | null,
): boolean {
  const left = a || {};
  const right = b || {};
  const keys = Object.keys(left);
  if (keys.length !== Object.keys(right).length) return false;
  return keys.every((k) => left[k] === right[k]);
}

export function applyEnv(input: string, env: Record<string, string>): string {
  return input.replace(/\{\{\s*([\w.-]+)\s*\}\}/g, (_, name: string) =>
    Object.prototype.hasOwnProperty.call(env, name) ? env[name] : `{{${name}}}`,
  );
}

const EXECUTE_PLACEHOLDER_PREFIXES = ["request.", "response.", "credential."];

function isExecutePlaceholder(name: string): boolean {
  return EXECUTE_PLACEHOLDER_PREFIXES.some((p) => name.startsWith(p));
}

function pushKvParts(
  parts: string[],
  rows?: Array<{ key?: string; value?: string; enabled?: boolean }> | Record<string, string> | null,
) {
  if (!rows) return;
  if (Array.isArray(rows)) {
    for (const row of rows) {
      if (row.enabled === false) continue;
      if (row.key) parts.push(row.key);
      if (row.value) parts.push(row.value);
    }
    return;
  }
  for (const [k, v] of Object.entries(rows)) {
    parts.push(k, String(v ?? ""));
  }
}

/** WORKBENCH_ENV_SURFACE — keep in lockstep with mgmt `workbench_spec.rs`. */
export function workbenchEnvSurfaceParts(args: {
  upstreamUrl?: string | null;
  endpoint?: ConnectorEndpoint | null;
  authFlow?: {
    token_url?: string;
    method?: string;
    inject_to?: string;
    inject_key?: string;
    inject_value_format?: string;
    request_body_tpl?: string;
    headers?: Array<{ key?: string; value?: string; enabled?: boolean }>;
  } | null;
  transform?: {
    rewrite_path?: string;
    request_body_template?: string;
    response_body_template?: string;
    request_headers_set?: Array<{ key?: string; value?: string; enabled?: boolean }>;
    response_headers_set?: Array<{ key?: string; value?: string; enabled?: boolean }>;
    request_headers_remove?: string[];
  } | null;
}): string[] {
  const parts: string[] = [];
  if (args.upstreamUrl) parts.push(args.upstreamUrl);
  const ep = args.endpoint;
  if (ep) {
    for (const s of [
      ep.path,
      ep.method,
      ep.operation,
      ep.wrap,
      ep.unwrap,
      ep.base_url,
    ]) {
      if (s) parts.push(s);
    }
    pushKvParts(parts, ep.header_entries || ep.headers);
    pushKvParts(parts, ep.query_entries || ep.query);
    if (typeof ep.body === "string") parts.push(ep.body);
    else if (ep.body != null) parts.push(JSON.stringify(ep.body));
    const gql = ep.graphql || {};
    if (typeof gql.document === "string") parts.push(gql.document);
    if (typeof gql.operation_name === "string") parts.push(gql.operation_name);
    pushKvParts(parts, gql.variables_map as Record<string, string> | undefined);
    pushKvParts(parts, gql.enum_map as Record<string, string> | undefined);
    const jp = ep.job_poll || {};
    if (typeof jp.submit_path === "string") parts.push(jp.submit_path);
    if (typeof jp.query_path === "string") parts.push(jp.query_path);
    const ws = ep.ws || {};
    if (typeof ws.path === "string") parts.push(ws.path);
    if (ep.soap) parts.push(JSON.stringify(ep.soap));
    if (ep.sign_form) parts.push(JSON.stringify(ep.sign_form));
  }
  const auth = args.authFlow;
  if (auth) {
    for (const s of [
      auth.token_url,
      auth.method,
      auth.inject_to,
      auth.inject_key,
      auth.inject_value_format,
      auth.request_body_tpl,
    ]) {
      if (s) parts.push(s);
    }
    pushKvParts(parts, auth.headers);
  }
  const tr = args.transform;
  if (tr) {
    if (tr.rewrite_path) parts.push(tr.rewrite_path);
    if (tr.request_body_template) parts.push(tr.request_body_template);
    if (tr.response_body_template) parts.push(tr.response_body_template);
    pushKvParts(parts, tr.request_headers_set);
    pushKvParts(parts, tr.response_headers_set);
    for (const s of tr.request_headers_remove || []) {
      if (s) parts.push(s);
    }
  }
  return parts;
}

export function findUnresolvedVars(
  parts: Array<string | undefined | null>,
  env: Record<string, string>,
): string[] {
  const found = new Set<string>();
  for (const part of parts) {
    if (!part) continue;
    const re = /\{\{\s*([\w.-]+)\s*\}\}/g;
    let m: RegExpExecArray | null;
    while ((m = re.exec(part))) {
      const name = m[1];
      if (!name || isExecutePlaceholder(name)) continue;
      if (!Object.prototype.hasOwnProperty.call(env, name)) found.add(name);
    }
  }
  return [...found];
}

export function formatTime(iso: string) {
  try {
    return new Date(iso).toLocaleString();
  } catch {
    return iso;
  }
}


export function formatBytes(bytes: number): string {
  const n = Number.isFinite(bytes) && bytes > 0 ? bytes : 0;
  if (n < 1000) return `${Math.round(n)} B`;
  if (n < 1_000_000) return `${(n / 1000).toFixed(2)} KB`;
  return `${(n / 1_000_000).toFixed(2)} MB`;
}

export function byteSizeOf(text: string): number {
  return new TextEncoder().encode(text ?? "").byteLength;
}

export const JSON_TREE_MAX_DEPTH = 12;
export const JSON_TREE_MAX_NODES = 2000;

export function jsonTreeFits(
  value: unknown,
  maxDepth = JSON_TREE_MAX_DEPTH,
  maxNodes = JSON_TREE_MAX_NODES,
): boolean {
  let nodes = 0;
  function walk(v: unknown, depth: number): boolean {
    if (depth > maxDepth) return false;
    nodes += 1;
    if (nodes > maxNodes) return false;
    if (v && typeof v === "object") {
      const kids = Array.isArray(v) ? v : Object.values(v as Record<string, unknown>);
      for (const child of kids) {
        if (!walk(child, depth + 1)) return false;
      }
    }
    return true;
  }
  return walk(value, 0);
}

export type ResponseBodyKind = "json" | "xml" | "raw";

export function detectResponseBodyKind(
  body: string,
  adapter?: string,
): ResponseBodyKind {
  if ((adapter || "") === "soap_xml") return "xml";
  const t = (body || "").trim();
  if (!t) return "raw";
  try {
    JSON.parse(t);
    return "json";
  } catch {
    void 0;
  }
  if (t.startsWith("<")) return "xml";
  return "raw";
}

export function tryParseJsonValue(raw: string): { ok: true; value: unknown } | { ok: false } {
  const t = (raw || "").trim();
  if (!t) return { ok: false };
  try {
    return { ok: true, value: JSON.parse(t) };
  } catch {
    return { ok: false };
  }
}

export function genSessionCode() {
  return `sess-${Date.now().toString(36)}`;
}

const PERSISTED_DRAFT_ID =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export function isPersistedDraftId(id?: string | null): boolean {
  return Boolean(id && PERSISTED_DRAFT_ID.test(id));
}

export function methodTone(method?: string): string {
  switch ((method || "GET").toUpperCase()) {
    case "GET":
      return "get";
    case "POST":
      return "post";
    case "PUT":
      return "put";
    case "PATCH":
      return "patch";
    case "DELETE":
      return "delete";
    case "HEAD":
      return "head";
    case "OPTIONS":
      return "options";
    default:
      return "default";
  }
}

export function methodColorVar(method?: string): string {
  return `var(--wb-method-${methodTone(method)})`;
}

export type AdapterBodyKind =
  | "json"
  | "form"
  | "xml"
  | "graphql"
  | "grpc"
  | "job_poll";

export function adapterBodyKind(adapter?: string): AdapterBodyKind {
  switch (adapter) {
    case "sign_form":
      return "form";
    case "soap_xml":
      return "xml";
    case "graphql":
      return "graphql";
    case "grpc":
      return "grpc";
    case "job_poll":
      return "job_poll";
    default:
      return "json";
  }
}

export function defaultBodyContentType(ep: {
  method?: string;
  adapter?: string;
  body_content_type?: string;
  body_encoding?: string;
}): BodyContentType {
  const stored = (ep.body_content_type || "").trim().toLowerCase();
  if (stored === "none") return "none";
  if (stored === "json") return "json";
  const enc = (ep.body_encoding || "").trim().toLowerCase();
  if (enc === "none") return "none";
  if (enc === "form" || enc === "json") return "json";
  const method = (ep.method || "GET").toUpperCase();
  if (method === "GET" || method === "HEAD") return "none";
  return "json";
}

export function resolvedBodyEncoding(ep: {
  body_encoding?: string;
  body_content_type?: string;
  method?: string;
}): BodyEncoding {
  const enc = (ep.body_encoding || "").trim().toLowerCase();
  if (enc === "form" || enc === "none" || enc === "json") return enc;
  return defaultBodyContentType(ep) === "none" ? "none" : "json";
}

export function setTenantBodyKind(
  ep: ConnectorEndpoint,
  kind: BodyContentType,
) {
  ep.body_content_type = kind;
  if (kind === "none") {
    ep.body = null;
    ep.body_encoding = "none";
    return;
  }
  if (ep.body == null) ep.body = {};
  if ((ep.body_encoding || "").toLowerCase() === "none") {
    ep.body_encoding = "json";
  }
}

export function setBodyEncoding(ep: ConnectorEndpoint, enc: BodyEncoding) {
  ep.body_encoding = enc;
  if (enc === "none") {
    setTenantBodyKind(ep, "none");
    return;
  }
  if (defaultBodyContentType(ep) === "none" || ep.body == null) {
    ep.body_content_type = "json";
    if (ep.body == null) ep.body = {};
  }
}

export function coerceEndpointJsonBody(ep: ConnectorEndpoint): boolean {
  if (ep.body == null || typeof ep.body !== "string") return true;
  const t = ep.body.trim();
  if (!t) {
    ep.body = defaultBodyContentType(ep) === "none" ? null : {};
    return true;
  }
  try {
    ep.body = JSON.parse(t);
    return true;
  } catch {
    return false;
  }
}

export function adapterBodyFilled(ep: ConnectorEndpoint): boolean {
  if (defaultBodyContentType(ep) === "none") return false;
  if (adapterBodyKind(ep.adapter) === "graphql") {
    if (String(ep.graphql?.document ?? "").trim().length > 0) return true;
  }
  if (adapterBodyKind(ep.adapter) === "grpc" && ep.artifact_id) return true;
  if (ep.body == null) return false;
  if (typeof ep.body === "string") {
    const t = ep.body.trim();
    return t.length > 0 && t !== "{}";
  }
  try {
    return JSON.stringify(ep.body) !== "{}";
  } catch {
    return true;
  }
}

export function bodyToEditorText(
  body: unknown,
  kind: "json" | "xml",
): string {
  if (kind === "xml") {
    if (body == null) return "";
    return typeof body === "string" ? body : String(body);
  }
  if (body == null) return "";
  if (typeof body === "string") return body;
  try {
    return JSON.stringify(body, null, 2);
  } catch {
    return String(body);
  }
}

export function editorTextToBody(
  text: string,
  kind: "json" | "xml",
): unknown {
  if (kind === "xml") return text;
  const t = text.trim();
  if (!t) return undefined;
  try {
    return JSON.parse(t);
  } catch {
    return text;
  }
}

export function bodyAsFormRows(body: unknown): KvRow[] {
  if (body && typeof body === "object" && !Array.isArray(body)) {
    return mapToKv(recordFromUnknown(body));
  }
  if (typeof body === "string" && body.trim()) {
    try {
      const parsed = JSON.parse(body);
      if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
        return mapToKv(recordFromUnknown(parsed));
      }
    } catch {
      void 0;
    }
  }
  return [];
}

export function prettyJson(input: unknown): string {
  if (input == null) return "";
  if (typeof input === "string") {
    const t = input.trim();
    if (!t) return "";
    try {
      return JSON.stringify(JSON.parse(t), null, 2);
    } catch {
      return input;
    }
  }
  try {
    return JSON.stringify(input, null, 2);
  } catch {
    return String(input);
  }
}

export function tryParseJson(raw: string): unknown {
  const t = raw.trim();
  if (!t) return undefined;
  try {
    return JSON.parse(t);
  } catch {
    return raw;
  }
}

export function envFromRows(rows: KvRow[]): Record<string, string> {
  return kvToMap(rows);
}

export type ParsedImportEndpoint = {
  id: string;
  name: string;
  method: string;
  path: string;
  headers?: Record<string, string>;
  query?: Record<string, string>;
  body?: unknown;
  assertions: AssertionSpec[];
  gate: boolean;
  tag?: string;
};

export type ParsedImport = {
  name?: string;
  upstream_url?: string;
  endpoints: ParsedImportEndpoint[];
};

function defaultAssertions(): AssertionSpec[] {
  return [];
}

export function shellSplit(s: string): string[] {
  const out: string[] = [];
  let cur = "";
  let inSingle = false;
  let inDouble = false;
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if (c === "'" && !inDouble) {
      inSingle = !inSingle;
      continue;
    }
    if (c === '"' && !inSingle) {
      inDouble = !inDouble;
      continue;
    }
    if ((c === " " || c === "\t" || c === "\n" || c === "\r") && !inSingle && !inDouble) {
      if (cur) {
        out.push(cur);
        cur = "";
      }
      continue;
    }
    if (c === "\\" && !inSingle) {
      i += 1;
      if (i < s.length) cur += s[i];
      continue;
    }
    cur += c;
  }
  if (cur) out.push(cur);
  return out;
}

export function parseCurl(curl: string): ParsedImport {
  const tokens = shellSplit(curl.trim());
  let method = "GET";
  let url = "";
  const headers: Record<string, string> = {};
  let bodyRaw: string | undefined;

  for (let i = 0; i < tokens.length; i++) {
    const t = tokens[i];
    if (t === "curl") continue;
    if (t === "-X" || t === "--request") {
      method = (tokens[i + 1] || "GET").toUpperCase();
      i += 1;
      continue;
    }
    if (t === "-H" || t === "--header") {
      const h = tokens[i + 1] || "";
      const idx = h.indexOf(":");
      if (idx > 0) headers[h.slice(0, idx).trim()] = h.slice(idx + 1).trim();
      i += 1;
      continue;
    }
    if (t === "-d" || t === "--data" || t === "--data-raw" || t === "--data-binary") {
      bodyRaw = tokens[i + 1];
      if (method === "GET") method = "POST";
      i += 1;
      continue;
    }
    if (t.startsWith("-X") && t.length > 2) {
      method = t.slice(2).toUpperCase();
      continue;
    }
    if (!t.startsWith("-") && (t.startsWith("http://") || t.startsWith("https://"))) {
      url = t;
    }
  }
  if (!url) throw new Error("无法从 cURL 解析 URL");

  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    throw new Error("URL 无效");
  }
  const upstream_url = `${parsed.protocol}//${parsed.host}`;
  const path = parsed.pathname || "/";
  const query: Record<string, string> = {};
  parsed.searchParams.forEach((v, k) => {
    query[k] = v;
  });
  const body = bodyRaw != null ? tryParseJson(bodyRaw) : undefined;
  return {
    upstream_url,
    endpoints: [
      {
        id: "imported",
        name: `${method} ${path}`,
        method,
        path,
        headers,
        query,
        body,
        assertions: defaultAssertions(),
        gate: true,
      },
    ],
  };
}


export function tryParseCurlPaste(text: string): ParsedImport | null {
  const t = (text || "").trim();
  if (!/\bcurl\s/i.test(t)) return null;
  try {
    return parseCurl(t);
  } catch {
    return null;
  }
}

function parsePostmanUrl(url: unknown): {
  base: string;
  path: string;
  query: Record<string, string>;
} {
  const query: Record<string, string> = {};
  if (typeof url === "string") {
    try {
      const u = new URL(url);
      u.searchParams.forEach((v, k) => {
        query[k] = v;
      });
      return {
        base: `${u.protocol}//${u.host}`,
        path: u.pathname || "/",
        query,
      };
    } catch {
      return { base: "", path: "/", query };
    }
  }
  if (!url || typeof url !== "object") return { base: "", path: "/", query };
  const rec = url as Record<string, unknown>;
  let base = "";
  if (Array.isArray(rec.host)) {
    const host = rec.host.filter((x) => typeof x === "string").join(".");
    const protocol = String(rec.protocol || "https").replace(/:$/, "");
    base = `${protocol}://${host}`;
  }
  let path = "/";
  if (Array.isArray(rec.path)) {
    path = `/${rec.path.filter((x) => typeof x === "string").join("/")}`;
  } else if (typeof rec.path === "string") {
    path = rec.path.startsWith("/") ? rec.path : `/${rec.path}`;
  }
  if (Array.isArray(rec.query)) {
    for (const q of rec.query) {
      if (q && typeof q === "object") {
        const row = q as { key?: string; value?: string };
        if (row.key) query[row.key] = row.value ?? "";
      }
    }
  }
  return { base, path, query };
}

export function parsePostmanCollection(collection: unknown): ParsedImport {
  const root = (collection || {}) as Record<string, unknown>;
  const name =
    (root.info as { name?: string } | undefined)?.name || "Postman Import";
  const endpoints: ParsedImportEndpoint[] = [];
  let baseUrl = "";

  function walk(items: unknown, depth: number) {
    if (depth > 8 || !Array.isArray(items)) return;
    for (const item of items) {
      if (!item || typeof item !== "object") continue;
      const rec = item as Record<string, unknown>;
      if (rec.item) {
        walk(rec.item, depth + 1);
        continue;
      }
      const req = rec.request;
      if (!req) continue;
      const request =
        typeof req === "string"
          ? { method: "GET", url: req }
          : (req as Record<string, unknown>);
      const method = String(request.method || "GET").toUpperCase();
      const epName = String(rec.name || "request");
      const parsed = parsePostmanUrl(request.url);
      if (!baseUrl && parsed.base) baseUrl = parsed.base;
      const headers: Record<string, string> = {};
      if (Array.isArray(request.header)) {
        for (const h of request.header) {
          if (h && typeof h === "object") {
            const row = h as { key?: string; value?: string };
            if (row.key) headers[row.key] = row.value ?? "";
          }
        }
      }
      let body: unknown;
      const bodyObj = request.body as { raw?: string } | undefined;
      if (bodyObj?.raw) body = tryParseJson(bodyObj.raw);
      endpoints.push({
        id: `ep-${endpoints.length + 1}`,
        name: epName,
        method,
        path: parsed.path,
        headers,
        query: parsed.query,
        body,
        assertions: defaultAssertions(),
        gate: true,
      });
    }
  }
  walk(root.item, 0);
  return {
    name,
    upstream_url: baseUrl || undefined,
    endpoints,
  };
}

export function parseOpenApiDocument(document: unknown): ParsedImport {
  const root = (document || {}) as Record<string, unknown>;
  const info = (root.info || {}) as { title?: string };
  const name = info.title || "OpenAPI Import";
  let upstream_url: string | undefined;
  if (Array.isArray(root.servers) && root.servers[0]) {
    const u = (root.servers[0] as { url?: string }).url;
    if (u) upstream_url = u.replace(/\/$/, "");
  }
  const endpoints: ParsedImportEndpoint[] = [];
  const paths = (root.paths || {}) as Record<string, unknown>;
  const methods = ["get", "post", "put", "patch", "delete", "head", "options"];
  for (const [path, item] of Object.entries(paths)) {
    if (!item || typeof item !== "object") continue;
    const obj = item as Record<string, unknown>;
    for (const method of methods) {
      const op = obj[method];
      if (!op || typeof op !== "object") continue;
      const rec = op as {
        operationId?: string;
        summary?: string;
        tags?: unknown;
      };
      endpoints.push({
        id: `${method}-${endpoints.length + 1}`,
        name: rec.operationId || rec.summary || method,
        method: method.toUpperCase(),
        path,
        assertions: defaultAssertions(),
        gate: true,
        tag: firstOpenApiTag(rec.tags),
      });
    }
  }
  endpoints.sort((a, b) => {
    const ta = a.tag || "\uFFFF";
    const tb = b.tag || "\uFFFF";
    if (ta !== tb) return ta.localeCompare(tb);
    return 0;
  });
  return { name, upstream_url, endpoints };
}

export function firstOpenApiTag(tags: unknown): string | undefined {
  if (!Array.isArray(tags)) return undefined;
  for (const t of tags) {
    if (typeof t === "string" && t.trim()) return t.trim();
  }
  return undefined;
}

export function defaultGraphqlSpec(): Record<string, unknown> {
  return {
    document: "",
    operation_name: "",
    variables_map: {},
    enum_map: {},
  };
}

export function defaultJobPollSpec(): Record<string, unknown> {
  return {
    submit_path: "",
    query_path: "/jobs/{job_id}",
    job_id_jsonpath: "$.job_id",
    status_jsonpath: "$.status",
    result_jsonpath: "$.result",
    message_jsonpath: "$.message",
    poll_interval_ms: 500,
  };
}

export function ensureEndpointAdapterFields(ep: ConnectorEndpoint): ConnectorEndpoint {
  const adapter = ep.adapter || "http_json";
  ep.adapter = adapter;
  if (adapter === "graphql") {
    ep.graphql = { ...defaultGraphqlSpec(), ...(ep.graphql || {}) };
  }
  if (adapter === "job_poll") {
    ep.job_poll = { ...defaultJobPollSpec(), ...(ep.job_poll || {}) };
  }
  if (!ep.auth) ep.auth = { profile: "none" };
  return ep;
}

export function normalizeWorkbenchEndpoint(
  ep: ConnectorEndpoint,
  index: number,
): ConnectorEndpoint {
  return ensureEndpointAdapterFields({
    ...ep,
    id: ep.id || `ep-${index + 1}`,
    method: ep.method || "GET",
    path: typeof ep.path === "string" ? ep.path : "/",
    name: ep.name || ep.path || `ep-${index + 1}`,
    headers: ep.headers || {},
    query: ep.query || {},
    assertions: Array.isArray(ep.assertions) ? ep.assertions : [],
    gate: ep.gate !== false,
    adapter: ep.adapter || "http_json",
  });
}

export function blankWorkbenchEndpoint(index = 0): ConnectorEndpoint {
  return normalizeWorkbenchEndpoint(
    {
      id: `ep-${Date.now().toString(36)}`,
      name: `endpoint-${index + 1}`,
      method: "GET",
      path: "",
      adapter: "http_json",
      assertions: [],
      gate: true,
    },
    index,
  );
}

export async function fileToBase64(file: File): Promise<string> {
  const buf = await file.arrayBuffer();
  const bytes = new Uint8Array(buf);
  const chunk = 0x8000;
  let binary = "";
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

export function recordFromUnknown(value: unknown): Record<string, string> {
  if (!value || typeof value !== "object" || Array.isArray(value)) return {};
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
    if (k) out[k] = v == null ? "" : String(v);
  }
  return out;
}

export function toConnectorEndpoint(
  parsed: ParsedImportEndpoint,
  index: number,
): ConnectorEndpoint {
  return {
    id: parsed.id || `ep-${index + 1}`,
    name: parsed.name,
    method: parsed.method,
    path: parsed.path,
    headers: parsed.headers || {},
    query: parsed.query || {},
    body: parsed.body,
    assertions: parsed.assertions?.length ? parsed.assertions : defaultAssertions(),
    gate: parsed.gate !== false,
    tag: parsed.tag,
  };
}

export function mergeImportedEndpoints(
  current: ConnectorEndpoint[],
  incoming: ParsedImportEndpoint[],
): ConnectorEndpoint[] {
  const next = [...current];
  incoming.forEach((ep, i) => {
    const mapped = toConnectorEndpoint(ep, next.length + i);
    const clash = next.some((e) => e.id === mapped.id);
    if (clash) mapped.id = `ep-${Date.now().toString(36)}-${next.length + i}`;
    next.push(mapped);
  });
  return next;
}

export type ChannelDraft = {
  endpoint_id?: string;
  channel_code: string;
  name: string;
  path_prefix: string;
  qps_limit: number;
};

export function autoChannelsFromEndpoints(
  endpoints: ConnectorEndpoint[],
): ChannelDraft[] {
  if (!endpoints.length) {
    return [
      {
        channel_code: "DEFAULT",
        name: "default",
        path_prefix: "/",
        qps_limit: 0,
      },
    ];
  }
  return endpoints.map((ep, i) => {
    const name = ep.name || ep.path || "endpoint";
    const path = ep.path || "/";
    const code = (ep.channel_code || "").trim() || suggestChannelCode(ep, i);
    return {
      endpoint_id: ep.id,
      channel_code: code,
      name,
      path_prefix: (ep.path_prefix || path || "/").trim() || "/",
      qps_limit: 0,
    };
  });
}

export function suggestChannelCode(
  ep: Pick<ConnectorEndpoint, "name" | "path">,
  index: number,
): string {
  const name = ep.name || ep.path || "endpoint";
  return (
    name
      .replace(/[^A-Za-z0-9]/g, "")
      .toUpperCase()
      .slice(0, 20) || `EP${index + 1}`
  );
}

export type RunView = {
  status?: number;
  url?: string;
  headers?: Record<string, string>;
  body?: string;
};

function asStageList(run: ConnectorRun | null | undefined): StageResult[] {
  const stages = run?.stages;
  return Array.isArray(stages) ? stages : [];
}

export function extractRunResponse(run: ConnectorRun | null | undefined): RunView {
  if (!run) return {};
  const preview = run.response_preview;
  if (preview && (preview.body_preview || preview.status != null || preview.url)) {
    return {
      status: preview.status,
      url: preview.url,
      headers: preview.headers,
      body: preview.body_preview,
    };
  }
  const stages = asStageList(run);
  const prefer = ["response_transform", "upstream"];
  const picked =
    stages.find((s) => prefer.includes(s.name) && s.response_summary) ||
    [...stages].reverse().find((s) => s.response_summary);
  const rs = picked?.response_summary || {};
  const headers = (rs.headers_masked || rs.headers) as Record<string, string> | undefined;
  return {
    status: typeof rs.status === "number" ? rs.status : undefined,
    url: typeof rs.url === "string" ? rs.url : undefined,
    headers,
    body: typeof rs.body_preview === "string" ? rs.body_preview : undefined,
  };
}

export function extractPipeline(run: ConnectorRun | null | undefined) {
  const stages = asStageList(run);
  const byName = (name: string) => stages.find((s) => s.name === name);
  const auth = byName("auth");
  const reqTx = byName("request_transform");
  const upstream = byName("upstream");
  const resTx = byName("response_transform");
  return {
    stages,
    auth,
    transformedRequest: reqTx?.request_summary,
    upstreamResponse: upstream?.response_summary,
    tenantResponse: resTx?.response_summary || extractRunResponse(run),
  };
}

export type GateEpStatus = {
  id: string;
  name: string;
  method: string;
  path: string;
  gated: boolean;
  ok: boolean;
  lastOkAt?: string;
  reason?: string;
  fingerprint?: string | null;
};

/** Map GET draft.gate[] onto the current endpoint list. SPA does not hash. */
export function gateViewFromServer(
  endpoints: ConnectorEndpoint[],
  server: DraftGateItem[] | undefined,
): { ok: number; total: number; items: GateEpStatus[] } {
  const byKey = new Map<string, DraftGateItem>();
  for (const g of server || []) {
    if (g.endpoint_key) byKey.set(g.endpoint_key, g);
  }
  const items: GateEpStatus[] = endpoints.map((ep, i) => {
    const id = ep.id || `ep-${i + 1}`;
    const aliases = [ep.id, ep.name, ep.path, id].filter(
      (s): s is string => Boolean(s && s.length),
    );
    let hit: DraftGateItem | undefined;
    for (const a of aliases) {
      hit = byKey.get(a);
      if (hit) break;
    }
    const gated = ep.gate !== false && hit?.reason !== "skipped";
    const met = hit?.met === true;
    return {
      id,
      name: ep.name || ep.path || id,
      method: ep.method || "GET",
      path: ep.path || "/",
      gated,
      ok: !gated || met,
      reason: hit?.reason,
      fingerprint: hit?.fingerprint ?? null,
    };
  });
  const gatedItems = items.filter((x) => x.gated);
  return {
    ok: gatedItems.filter((x) => x.ok).length,
    total: gatedItems.length,
    items,
  };
}

const TOKENISH_HEADER =
  /^(authorization|proxy-authorization|x-api-key|api-key|x-auth-token|x-access-token)$/i;

/** Headers that still look like tokens after auth_flow inject_key strip (hint only). */
export function leftoverTokenHeaderNames(args: {
  endpoints: ConnectorEndpoint[];
  authEnabled: boolean;
  injectTo?: string;
  injectKey?: string;
}): string[] {
  if (!args.authEnabled) return [];
  const injectTo = (args.injectTo || "header").trim().toLowerCase();
  const injectKey = (args.injectKey || "Authorization").trim().toLowerCase();
  const found = new Set<string>();
  for (const ep of args.endpoints) {
    const names = ep.header_entries?.length
      ? ep.header_entries
          .filter((r) => r.enabled !== false && r.key.trim())
          .map((r) => r.key)
      : Object.keys(ep.headers || {});
    for (const rawKey of names) {
      const key = rawKey.trim();
      if (!key) continue;
      if (injectTo === "header" && key.toLowerCase() === injectKey) continue;
      if (TOKENISH_HEADER.test(key)) found.add(key);
    }
  }
  return [...found];
}

export function buildCurl(opts: {
  baseUrl: string;
  method: string;
  path: string;
  headers?: Record<string, string>;
  query?: Record<string, string>;
  body?: unknown;
}): string {
  const u = new URL(
    (opts.path || "/").startsWith("http")
      ? opts.path
      : `${(opts.baseUrl || "http://localhost").replace(/\/$/, "")}${opts.path || "/"}`,
  );
  Object.entries(opts.query || {}).forEach(([k, v]) => u.searchParams.set(k, v));
  const parts = [`curl -sS -X ${opts.method || "GET"} '${u.toString()}'`];
  Object.entries(opts.headers || {}).forEach(([k, v]) => {
    parts.push(`  -H '${k}: ${v}'`);
  });
  if (opts.body != null && opts.body !== "") {
    const raw = typeof opts.body === "string" ? opts.body : JSON.stringify(opts.body);
    parts.push(`  -d '${raw.replace(/'/g, `'\\''`)}'`);
  }
  return parts.join(" \\\n");
}

export function lastAuthJson(run: ConnectorRun | null | undefined): string {
  const stages = asStageList(run);
  const auth = stages.find((s) => s.name === "auth");
  const body = auth?.response_summary?.body_preview;
  return typeof body === "string" ? body : "";
}
