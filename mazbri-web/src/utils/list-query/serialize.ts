import type { ListFilter } from "@/types/list-query";

export type ListQueryEncodeMode = "json" | "flat" | "both";







export function encodeFilters(
  filters: ListFilter[] | undefined,
  mode: ListQueryEncodeMode = "both",
): Record<string, string> {
  if (!filters?.length) return {};
  const out: Record<string, string> = {};
  if (mode === "json" || mode === "both") {
    out.filters = JSON.stringify(filters);
  }
  if (mode === "flat" || mode === "both") {
    for (const f of filters) {
      Object.assign(out, encodeFlatFilter(f));
    }
  }
  return out;
}

function encodeFlatFilter(f: ListFilter): Record<string, string> {
  const { field, op, value } = f;
  if (op === "is_null") return { [`${field}_is_null`]: "1" };
  if (op === "is_not_null") return { [`${field}_is_not_null`]: "1" };
  if (value === undefined || value === null) return {};

  if (op === "between" && Array.isArray(value) && value.length === 2) {
    return {
      [`${field}_gte`]: String(value[0]),
      [`${field}_lte`]: String(value[1]),
    };
  }
  if (op === "in" && Array.isArray(value)) {
    return { [`${field}_in`]: value.map(String).join(",") };
  }
  return { [`${field}_${op}`]: String(value) };
}


export function buildListQueryParams(options: {
  q?: string;
  filters?: ListFilter[];
  encodeMode?: ListQueryEncodeMode;

  extra?: Record<string, unknown>;
}): Record<string, unknown> {
  const params: Record<string, unknown> = { ...(options.extra ?? {}) };
  const q = options.q?.trim();
  if (q) {
    params.q = q;
  }
  Object.assign(
    params,
    encodeFilters(options.filters, options.encodeMode ?? "both"),
  );
  return params;
}


export function formatFilterChipText(options: {
  fieldLabel: string;
  opLabel: string;
  value: unknown;
  unit?: string;
}): string {
  const { fieldLabel, opLabel, value, unit } = options;
  if (value === undefined || value === null || value === "") {
    return `${fieldLabel} ${opLabel}`;
  }
  if (Array.isArray(value)) {
    const joined = value.map(String).join(" – ");
    return `${fieldLabel} ${opLabel} ${joined}${unit ? ` ${unit}` : ""}`;
  }
  return `${fieldLabel} ${opLabel} ${String(value)}${unit ? ` ${unit}` : ""}`;
}
