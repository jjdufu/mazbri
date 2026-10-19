import type {
  FilterFieldDef,
  FilterOp,
  ListFilter,
  ResourceFilterCatalog,
} from "@/types/list-query";
import { DEFAULT_MAX_FILTERS } from "@/types/list-query";

export function catalogQFields(catalog: ResourceFilterCatalog): FilterFieldDef[] {
  return catalog.fields.filter((f) => f.in_q && !f.ui_hidden);
}

export function catalogFilterableFields(
  catalog: ResourceFilterCatalog,
): FilterFieldDef[] {
  return catalog.fields.filter((f) => !f.ui_hidden);
}

export function findFieldDef(
  catalog: ResourceFilterCatalog,
  field: string,
): FilterFieldDef | undefined {
  return catalog.fields.find((f) => f.field === field);
}

export function maxFilters(catalog: ResourceFilterCatalog): number {
  return catalog.max_filters ?? DEFAULT_MAX_FILTERS;
}


export function buildQPlaceholder(
  catalog: ResourceFilterCatalog,
  t: (key: string) => string,
  fallback = "",
): string {
  const labels = catalogQFields(catalog)
    .map((f) => {
      try {
        const label = t(f.label_key);
        return label === f.label_key ? f.field : label;
      } catch {
        return f.field;
      }
    })
    .filter(Boolean);
  return labels.length > 0 ? labels.join(" / ") : fallback;
}

export function validateFilterAgainstCatalog(
  catalog: ResourceFilterCatalog,
  filter: ListFilter,
): string | null {
  const def = findFieldDef(catalog, filter.field);
  if (!def) {
    return `Unknown filter field: ${filter.field}`;
  }
  if (!def.ops.includes(filter.op)) {
    return `Operator ${filter.op} is not allowed for field ${filter.field}`;
  }
  if (filter.op === "is_null" || filter.op === "is_not_null") {
    return null;
  }
  if (filter.value === undefined || filter.value === null || filter.value === "") {
    return `Value required for ${filter.field} ${filter.op}`;
  }
  if (filter.op === "between") {
    if (!Array.isArray(filter.value) || filter.value.length !== 2) {
      return `between requires a 2-value array for ${filter.field}`;
    }
  }
  if (filter.op === "in") {
    if (!Array.isArray(filter.value) || filter.value.length === 0) {
      return `in requires a non-empty array for ${filter.field}`;
    }
  }
  return null;
}

export function validateFilters(
  catalog: ResourceFilterCatalog,
  filters: ListFilter[],
): string | null {
  if (filters.length > maxFilters(catalog)) {
    return `Too many filters (max ${maxFilters(catalog)})`;
  }
  for (const f of filters) {
    const err = validateFilterAgainstCatalog(catalog, f);
    if (err) return err;
  }
  return null;
}

export function opsForField(def: FilterFieldDef): FilterOp[] {
  return def.ops;
}

export function filtersRequireTimeWindow(
  catalog: ResourceFilterCatalog,
  filters: ListFilter[],
): boolean {
  const keys = new Set(
    catalog.require_time_window_when ??
      catalog.fields
        .filter((f) => f.requires_time_window)
        .map((f) => f.field),
  );
  return filters.some((f) => keys.has(f.field));
}
