import type { ListFilter } from "@/types/list-query";


export function filterFieldOpKey(f: Pick<ListFilter, "field" | "op">): string {
  return `${f.field}:${f.op}`;
}


export function filterExactKey(f: ListFilter): string {
  return `${f.field}:${f.op}:${JSON.stringify(f.value ?? null)}`;
}





export function mergeByFieldOp(
  filters: ListFilter[],
  next: ListFilter,
): ListFilter[] {
  const key = filterFieldOpKey(next);
  let replaced = false;
  const out = filters.map((f) => {
    if (filterFieldOpKey(f) === key) {
      replaced = true;
      return next;
    }
    return f;
  });
  if (!replaced) {
    out.push(next);
  }
  return out;
}





export function mergeFiltersByFieldOp(
  base: ListFilter[],
  incoming: ListFilter[],
): ListFilter[] {
  let out = [...base];
  for (const f of incoming) {
    out = mergeByFieldOp(out, f);
  }
  return out;
}


export function removeExactFilters(
  filters: ListFilter[],
  toRemove: ListFilter[],
): ListFilter[] {
  const drop = new Set(toRemove.map(filterExactKey));
  return filters.filter((f) => !drop.has(filterExactKey(f)));
}


export function isFilterSetActive(
  active: ListFilter[],
  set: ListFilter[],
): boolean {
  if (!set.length) return false;
  const keys = new Set(active.map(filterExactKey));
  return set.every((f) => keys.has(filterExactKey(f)));
}
