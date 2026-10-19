import type {
  FilterFieldDef,
  FilterOp,
  FilterValue,
  ListFilter,
  ResourceFilterCatalog,
} from "@/types/list-query";
import {
  buildAliasMaps,
  matchFieldRefAtStart,
  type AliasMaps,
  type TranslateFn,
} from "@/utils/list-query/field-aliases";
import { parseDatetimeInput } from "@/utils/list-query/parse-datetime-input";
import { parseDurationMsInput } from "@/utils/list-query/parse-duration-input";
import { parseRequestIdLookup } from "@/utils/list-query/request-id";

export type ParseErrorCode =
  | "unknown_field"
  | "op_not_allowed"
  | "invalid_value"
  | "invalid_number"
  | "invalid_duration"
  | "invalid_datetime"
  | "invalid_decimal"
  | "invalid_enum"
  | "invalid_bool"
  | "invalid_uuid"
  | "empty_value"
  | "unsupported_syntax"
  | "use_builder"
  | "ambiguous_label";

export type ParseOk = { ok: true; filter: ListFilter };
export type ParseErr = {
  ok: false;
  code: ParseErrorCode;
  messageKey: string;
  messageParams?: Record<string, string | number>;
};
export type ParseResult = ParseOk | ParseErr;


interface OpToken {
  token: string;
  op: FilterOp;

  word: boolean;
}

const OP_TOKENS: OpToken[] = [

  { token: "is not null", op: "is_not_null", word: true },
  { token: "is_not_null", op: "is_not_null", word: true },
  { token: "not empty", op: "is_not_null", word: true },
  { token: "非空", op: "is_not_null", word: true },
  { token: "is null", op: "is_null", word: true },
  { token: "is_null", op: "is_null", word: true },
  { token: "empty", op: "is_null", word: true },
  { token: "为空", op: "is_null", word: true },

  { token: "starts_with", op: "starts_with", word: true },
  { token: "ends_with", op: "ends_with", word: true },
  { token: "contains", op: "contains", word: true },
  { token: "前缀", op: "starts_with", word: true },
  { token: "后缀", op: "ends_with", word: true },
  { token: "包含", op: "contains", word: true },
  { token: "^=", op: "starts_with", word: false },
  { token: ">=", op: "gte", word: false },
  { token: "<=", op: "lte", word: false },
  { token: "≠", op: "neq", word: false },
  { token: "≥", op: "gte", word: false },
  { token: "≤", op: "lte", word: false },
  { token: "!=", op: "neq", word: false },
  { token: "<>", op: "neq", word: false },
  { token: "==", op: "eq", word: false },
  { token: "gte", op: "gte", word: true },
  { token: "lte", op: "lte", word: true },
  { token: "neq", op: "neq", word: true },
  { token: "eq", op: "eq", word: true },
  { token: "gt", op: "gt", word: true },
  { token: "lt", op: "lt", word: true },
  { token: "~", op: "contains", word: false },
  { token: ">", op: "gt", word: false },
  { token: "<", op: "lt", word: false },
  { token: "=", op: "eq", word: false },
  { token: ":", op: "eq", word: false },

  { token: "between", op: "between", word: true },
  { token: "in", op: "in", word: true },
];


OP_TOKENS.sort((a, b) => b.token.length - a.token.length);

export interface OpMatch {
  op: FilterOp;
  token: string;
  endIndex: number;
}





export function matchOperatorAtStart(rest: string): OpMatch | null {
  const s = rest.replace(/^\s+/, "");
  const leadingWs = rest.length - s.length;
  if (!s) return null;

  for (const entry of OP_TOKENS) {
    if (entry.word) {
      const slice = s.slice(0, entry.token.length);
      if (slice.toLowerCase() !== entry.token.toLowerCase()) continue;

      const after = s.slice(entry.token.length);
      if (after.length > 0 && !/^\s/.test(after) && !isValueStart(after[0]!)) {


        if (!/^\s/.test(after)) continue;
      }
      return {
        op: entry.op,
        token: entry.token,
        endIndex: leadingWs + entry.token.length,
      };
    }
    if (!s.startsWith(entry.token)) continue;
    return {
      op: entry.op,
      token: entry.token,
      endIndex: leadingWs + entry.token.length,
    };
  }
  return null;
}

function isValueStart(ch: string): boolean {
  return ch === '"' || ch === "'" || /[^\s]/.test(ch);
}






export function looksLikeStrongClause(
  input: string,
  catalog: ResourceFilterCatalog,
  t: TranslateFn,
  maps?: AliasMaps,
): boolean {
  const text = input.trim();
  if (!text) return false;
  const aliasMaps = maps ?? buildAliasMaps(catalog, t);
  const fieldMatch = matchFieldRefAtStart(text, aliasMaps);
  if (!fieldMatch) {

    return looksLikeOpWithUnknownField(text);
  }
  const rest = text.slice(fieldMatch.endIndex);
  if (!rest.trim()) return false;
  return matchOperatorAtStart(rest) !== null;
}





function looksLikeOpWithUnknownField(text: string): boolean {

  for (const entry of OP_TOKENS) {
    if (entry.op === "between" || entry.op === "in") continue;
    const idx = findTokenIndex(text, entry);
    if (idx <= 0) continue;
    const left = text.slice(0, idx).trim();
    if (!left) continue;

    return true;
  }
  return false;
}

function findTokenIndex(text: string, entry: OpToken): number {
  if (entry.word) {
    const lower = text.toLowerCase();
    const token = entry.token.toLowerCase();
    let from = 0;
    while (from < lower.length) {
      const idx = lower.indexOf(token, from);
      if (idx < 0) return -1;
      const beforeOk = idx === 0 || /\s/.test(text[idx - 1]!);
      const afterIdx = idx + token.length;
      const afterOk =
        afterIdx >= text.length ||
        /\s/.test(text[afterIdx]!) ||
        isValueStart(text[afterIdx]!);
      if (beforeOk && afterOk) {

        if (afterIdx < text.length && !/\s/.test(text[afterIdx]!)) {
          from = idx + 1;
          continue;
        }
        return idx;
      }
      from = idx + 1;
    }
    return -1;
  }
  return text.indexOf(entry.token);
}

function err(
  code: ParseErrorCode,
  params?: Record<string, string | number>,
): ParseErr {
  return {
    ok: false,
    code,
    messageKey: `listQueryBar.parse.${code}`,
    messageParams: params,
  };
}





export function parseClause(
  input: string,
  catalog: ResourceFilterCatalog,
  t: TranslateFn,
  maps?: AliasMaps,
): ParseResult {
  const text = input.trim();
  if (!text) {
    return err("empty_value");
  }

  const aliasMaps = maps ?? buildAliasMaps(catalog, t);
  const fieldMatch = matchFieldRefAtStart(text, aliasMaps);

  if (!fieldMatch) {

    if (looksLikeOpWithUnknownField(text)) {
      return err("unknown_field", { field: text.split(/\s+/)[0] ?? text });
    }
    return err("unknown_field", { field: text.split(/\s+/)[0] ?? text });
  }


  if (
    fieldMatch.kind === "label" &&
    aliasMaps.ambiguousLabels.has(fieldMatch.matchedText)
  ) {
    return err("ambiguous_label", { label: fieldMatch.matchedText });
  }

  const afterField = text.slice(fieldMatch.endIndex);
  if (!afterField.trim()) {
    return err("empty_value");
  }

  const opMatch = matchOperatorAtStart(afterField);
  if (!opMatch) {
    return err("unsupported_syntax");
  }

  const { op } = opMatch;
  const def = fieldMatch.def;

  if (op === "between" || op === "in") {
    return err("use_builder");
  }

  if (!def.ops.includes(op)) {
    return err("op_not_allowed", { field: def.field, op });
  }

  const valueRaw = afterField.slice(opMatch.endIndex).trim();

  if (op === "is_null" || op === "is_not_null") {
    if (valueRaw) {
      return err("invalid_value", { field: def.field });
    }
    return { ok: true, filter: { field: def.field, op } };
  }

  if (!valueRaw) {
    return err("empty_value");
  }

  const valueResult = parseValueExpr(def, valueRaw);
  if (!valueResult.ok) return valueResult;

  return {
    ok: true,
    filter: {
      field: def.field,
      op,
      value: valueResult.value,
    },
  };
}

type ValueOk = { ok: true; value: FilterValue };
type ValueResult = ValueOk | ParseErr;

function parseValueExpr(def: FilterFieldDef, raw: string): ValueResult {
  const unquoted = unquoteValue(raw);
  if (unquoted === null) {
    return err("invalid_value");
  }
  const { value: body, wasQuoted } = unquoted;
  if (!wasQuoted && body === "") {
    return err("empty_value");
  }

  switch (def.value_type) {
    case "int":
    case "uint":
      return parseNumeric(def, body);
    case "decimal":
      if (!body.trim()) return err("empty_value");
      return { ok: true, value: body.trim() };
    case "bool":
      return parseBool(body);
    case "enum":
      return parseEnum(def, body);
    case "uuid": {
      const id = parseRequestIdLookup(body) ?? parseLooseUuid(body);
      if (!id) return err("invalid_uuid");
      return { ok: true, value: id };
    }
    case "datetime": {
      const dt = parseDatetimeInput(body);
      if (!dt) return err("invalid_datetime");
      return { ok: true, value: dt };
    }
    case "string":
    default:
      if (!wasQuoted && !body) return err("empty_value");
      return { ok: true, value: body };
  }
}

function parseNumeric(def: FilterFieldDef, body: string): ValueResult {
  if (def.unit === "ms") {
    const ms = parseDurationMsInput(body);
    if (ms === null) return err("invalid_duration");
    if (def.value_type === "uint" && ms < 0) return err("invalid_number");
    return { ok: true, value: ms };
  }
  const trimmed = body.trim();
  if (!/^-?\d+$/.test(trimmed)) {

    const n = Number(trimmed);
    if (!Number.isFinite(n) || !Number.isInteger(n)) {
      return err("invalid_number");
    }
    if (def.value_type === "uint" && n < 0) return err("invalid_number");
    return { ok: true, value: n };
  }
  const n = Number(trimmed);
  if (!Number.isFinite(n) || !Number.isInteger(n)) {
    return err("invalid_number");
  }
  if (def.value_type === "uint" && n < 0) return err("invalid_number");
  return { ok: true, value: n };
}

function parseBool(body: string): ValueResult {
  const s = body.trim().toLowerCase();
  if (["true", "1", "yes", "是"].includes(s)) return { ok: true, value: true };
  if (["false", "0", "no", "否"].includes(s)) return { ok: true, value: false };
  return err("invalid_bool");
}

function parseEnum(def: FilterFieldDef, body: string): ValueResult {
  const values = def.enum_values ?? [];
  if (!values.includes(body)) {

    const trimmed = body.trim();
    if (!values.includes(trimmed)) {
      return err("invalid_enum", { value: body });
    }
    return { ok: true, value: trimmed };
  }
  return { ok: true, value: body };
}

function parseLooseUuid(body: string): string | null {
  const t = body.trim();
  if (
    /^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$/.test(
      t,
    )
  ) {
    return t.toLowerCase();
  }
  if (/^[0-9a-fA-F]{32}$/.test(t)) {
    const s = t.toLowerCase();
    return `${s.slice(0, 8)}-${s.slice(8, 12)}-${s.slice(12, 16)}-${s.slice(16, 20)}-${s.slice(20)}`;
  }
  return null;
}





export function unquoteValue(
  raw: string,
): { value: string; wasQuoted: boolean } | null {
  const s = raw.trim();
  if (!s) return { value: "", wasQuoted: false };
  const q = s[0];
  if (q !== '"' && q !== "'") {
    return { value: s, wasQuoted: false };
  }
  if (s.length < 2 || s[s.length - 1] !== q) {

    return null;
  }
  let out = "";
  for (let i = 1; i < s.length - 1; i++) {
    const ch = s[i]!;
    if (ch === "\\") {
      const next = s[i + 1];
      if (next === undefined) return null;
      out += next;
      i++;
      continue;
    }
    out += ch;
  }
  return { value: out, wasQuoted: true };
}


export function tryParseStrongClause(
  input: string,
  catalog: ResourceFilterCatalog,
  t: TranslateFn,
): ParseResult | null {
  const maps = buildAliasMaps(catalog, t);
  if (!looksLikeStrongClause(input, catalog, t, maps)) {
    return null;
  }
  return parseClause(input, catalog, t, maps);
}
