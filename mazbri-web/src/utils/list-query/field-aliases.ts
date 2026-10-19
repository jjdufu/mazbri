import type {
  FilterFieldDef,
  ResourceFilterCatalog,
} from "@/types/list-query";
import { catalogFilterableFields } from "@/utils/list-query/catalog";

export type TranslateFn = (key: string) => string;

export interface FieldRefCandidate {

  text: string;
  field: string;
  def: FilterFieldDef;
  kind: "api" | "label";
}

export interface AliasMaps {
  byField: Map<string, FilterFieldDef>;

  fieldRefs: FieldRefCandidate[];

  labelToField: Map<string, string>;

  ambiguousLabels: Set<string>;
}


export function normalizeLabelText(s: string): string {
  return s.trim().replace(/\s+/g, " ");
}





export function buildAliasMaps(
  catalog: ResourceFilterCatalog,
  t: TranslateFn,
): AliasMaps {
  const fields = catalogFilterableFields(catalog);
  const byField = new Map<string, FilterFieldDef>();
  const labelToField = new Map<string, string>();
  const ambiguousLabels = new Set<string>();
  const fieldRefs: FieldRefCandidate[] = [];

  for (const def of fields) {
    byField.set(def.field, def);
    fieldRefs.push({
      text: def.field,
      field: def.field,
      def,
      kind: "api",
    });

    let label: string;
    try {
      label = t(def.label_key);
    } catch {
      label = def.label_key;
    }
    if (!label || label === def.label_key) {
      continue;
    }
    const normalized = normalizeLabelText(label);
    if (!normalized || normalized === def.field) continue;

    const existing = labelToField.get(normalized);
    if (existing && existing !== def.field) {
      ambiguousLabels.add(normalized);
    } else {
      labelToField.set(normalized, def.field);
    }
    fieldRefs.push({
      text: normalized,
      field: def.field,
      def,
      kind: "label",
    });
  }


  fieldRefs.sort((a, b) => b.text.length - a.text.length);

  return { byField, fieldRefs, labelToField, ambiguousLabels };
}

export interface FieldRefMatch {
  def: FilterFieldDef;
  field: string;
  kind: "api" | "label";
  matchedText: string;

  endIndex: number;
}





function isValidAfterFieldRef(after: string): boolean {
  if (!after.length) return true;
  if (/^\s/.test(after)) return true;

  if (/^[=!<>:~]/.test(after)) return true;
  return false;
}





export function matchFieldRefAtStart(
  input: string,
  maps: AliasMaps,
): FieldRefMatch | null {
  const trimmedStart = input.match(/^\s*/)?.[0].length ?? 0;
  const body = input.slice(trimmedStart);
  if (!body) return null;




  for (const ref of maps.fieldRefs) {
    if (ref.kind === "api") {
      if (!body.startsWith(ref.text)) continue;
      const after = body.slice(ref.text.length);
      if (!isValidAfterFieldRef(after)) continue;
      return {
        def: ref.def,
        field: ref.field,
        kind: "api",
        matchedText: ref.text,
        endIndex: trimmedStart + ref.text.length,
      };
    }


    const normBody = normalizeLabelText(body);
    if (!normBody.startsWith(ref.text)) continue;

    if (maps.ambiguousLabels.has(ref.text)) continue;



    const consumed = consumeNormalizedPrefix(body, ref.text);
    if (consumed < 0) continue;
    const after = body.slice(consumed);
    if (!isValidAfterFieldRef(after)) continue;
    return {
      def: ref.def,
      field: ref.field,
      kind: "label",
      matchedText: ref.text,
      endIndex: trimmedStart + consumed,
    };
  }
  return null;
}





function consumeNormalizedPrefix(raw: string, normalizedPrefix: string): number {
  let ni = 0;
  let ri = 0;

  while (ri < raw.length && /\s/.test(raw[ri]!)) ri++;

  while (ni < normalizedPrefix.length && ri < raw.length) {
    const want = normalizedPrefix[ni]!;
    if (want === " ") {
      if (!/\s/.test(raw[ri]!)) return -1;
      while (ri < raw.length && /\s/.test(raw[ri]!)) ri++;
      ni++;
      continue;
    }
    if (raw[ri] !== want) return -1;
    ri++;
    ni++;
  }
  if (ni < normalizedPrefix.length) return -1;
  return ri;
}
