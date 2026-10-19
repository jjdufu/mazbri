import type {
  FilterOp,
  ListFilter,
  ResourceFilterCatalog,
} from "@/types/list-query";
import {
  buildAliasMaps,
  matchFieldRefAtStart,
  type AliasMaps,
  type TranslateFn,
} from "@/utils/list-query/field-aliases";
import { catalogFilterableFields, catalogQFields } from "@/utils/list-query/catalog";
import { matchOperatorAtStart } from "@/utils/list-query/parse-clause";


export type SuggestKind = "quick" | "field" | "op" | "enum" | "hint";

export type SuggestApply =
  | { type: "set_draft"; draft: string }
  | { type: "toggle_quick"; quickId: string; filters: ListFilter[] }
  | { type: "noop" };

export interface SuggestItem {
  id: string;
  kind: SuggestKind;
  label: string;
  detail?: string;
  apply: SuggestApply;
}

export interface SuggestOptions {
  catalog: ResourceFilterCatalog;
  t: TranslateFn;
  draftText: string;
  maxFields?: number;

  opLabel?: (op: FilterOp) => string;

  opDetail?: (op: FilterOp) => string | undefined;
  maps?: AliasMaps;
}

const DEFAULT_MAX_FIELDS = 48;

type Stage = "empty" | "field" | "op" | "value" | "free";

function resolveLabel(t: TranslateFn, key: string, fallback: string): string {
  try {
    const label = t(key);
    return label === key ? fallback : label;
  } catch {
    return fallback;
  }
}


function fieldDisplayName(
  def: { field: string; label_key: string },
  t: TranslateFn,
): string {
  return resolveLabel(t, def.label_key, def.field);
}

function detectStage(
  draft: string,
  maps: AliasMaps,
): {
  stage: Stage;
  field?: string;
  op?: FilterOp;
  fieldEnd?: number;
  opEnd?: number;
} {
  const text = draft;
  const trimmed = text.trim();
  if (!trimmed) return { stage: "empty" };

  const fieldMatch = matchFieldRefAtStart(text, maps);
  if (!fieldMatch) {
    return { stage: "field" };
  }

  const afterField = text.slice(fieldMatch.endIndex);
  if (!afterField.trim()) {
    return {
      stage: "op",
      field: fieldMatch.field,
      fieldEnd: fieldMatch.endIndex,
    };
  }

  const opMatch = matchOperatorAtStart(afterField);
  if (!opMatch) {
    return {
      stage: "op",
      field: fieldMatch.field,
      fieldEnd: fieldMatch.endIndex,
    };
  }

  if (opMatch.op === "is_null" || opMatch.op === "is_not_null") {
    return { stage: "free", field: fieldMatch.field, op: opMatch.op };
  }

  return {
    stage: "value",
    field: fieldMatch.field,
    op: opMatch.op,
    fieldEnd: fieldMatch.endIndex,
    opEnd: fieldMatch.endIndex + opMatch.endIndex,
  };
}










export function buildSuggestItems(options: SuggestOptions): SuggestItem[] {
  const {
    catalog,
    t,
    draftText,
    maxFields = DEFAULT_MAX_FIELDS,
  } = options;
  const maps = options.maps ?? buildAliasMaps(catalog, t);
  const opLabel =
    options.opLabel ??
    ((op: FilterOp) => resolveLabel(t, `listFilters.ops.${op}`, op));
  const opDetail = options.opDetail;

  const items: SuggestItem[] = [];
  const draft = draftText;
  const trimmed = draft.trim();
  const { stage, field, op } = detectStage(draft, maps);

  const pushFieldItem = (
    f: { field: string; label_key: string },
    draftValue?: string,
  ) => {
    const display = fieldDisplayName(f, t);
    items.push({
      id: `field:${f.field}`,
      kind: "field",
      label: display,
      apply: {
        type: "set_draft",
        draft: draftValue ?? `${display} `,
      },
    });
  };

  if (stage === "empty") {

    const qFields = catalogQFields(catalog);
    const rest = catalogFilterableFields(catalog).filter(
      (f) => !qFields.some((q) => q.field === f.field),
    );
    const fields = [...qFields, ...rest].slice(0, maxFields);
    for (const f of fields) {
      pushFieldItem(f);
    }
    if (!fields.length) {
      items.push({
        id: "hint:text",
        kind: "hint",
        label: resolveLabel(
          t,
          "listQueryBar.hintTextSearch",
          "Type text and press Enter to search",
        ),
        apply: { type: "noop" },
      });
    }
    return items;
  }

  if (stage === "field") {

    const needle = trimmed.toLowerCase();
    const fields = catalogFilterableFields(catalog)
      .filter((f) => {
        const label = resolveLabel(t, f.label_key, f.field).toLowerCase();
        return (
          f.field.toLowerCase().includes(needle) || label.includes(needle)
        );
      })
      .slice(0, maxFields);
    for (const f of fields) {
      pushFieldItem(f);
    }
    return items;
  }

  if (stage === "op" && field) {
    const def = maps.byField.get(field);

    if (def) {
      const fieldMatch = matchFieldRefAtStart(draft, maps);
      const afterField = draft.slice(fieldMatch?.endIndex ?? 0);
      const partial = afterField.trim().toLowerCase();
      for (const allowed of def.ops) {
        if (allowed === "between" || allowed === "in") continue;
        const label = preferredOpToken(allowed, opLabel);
        const detailText =
          opDetail?.(allowed) ??
          resolveLabel(
            t,
            `listQueryBar.opDetail.${allowed}`,
            opLabel(allowed),
          );
        if (
          partial &&
          !label.toLowerCase().includes(partial) &&
          !allowed.toLowerCase().includes(partial) &&
          !(detailText ?? "").toLowerCase().includes(partial) &&
          !partialOpMatches(allowed, partial)
        ) {
          continue;
        }
        items.push({
          id: `op:${field}:${allowed}`,
          kind: "op",
          label,
          detail: detailText,
          apply: {
            type: "set_draft",
            draft: buildDraftWithOp(draft, maps, field, allowed, opLabel),
          },
        });
      }
    }
    return items;
  }

  if (stage === "value" && field && op) {
    const def = maps.byField.get(field);
    if (def?.value_type === "enum" && def.enum_values?.length) {
      const afterOp = draft.slice(
        (matchFieldRefAtStart(draft, maps)?.endIndex ?? 0) +
          (matchOperatorAtStart(
            draft.slice(matchFieldRefAtStart(draft, maps)?.endIndex ?? 0),
          )?.endIndex ?? 0),
      );
      const partial = afterOp.trim().toLowerCase();
      def.enum_values.forEach((ev, idx) => {
        const elabel = def.enum_label_keys?.[idx]
          ? resolveLabel(t, def.enum_label_keys[idx]!, ev)
          : ev;
        if (
          partial &&
          !ev.toLowerCase().includes(partial) &&
          !elabel.toLowerCase().includes(partial)
        ) {
          return;
        }
        items.push({
          id: `enum:${field}:${ev}`,
          kind: "enum",
          label: elabel,
          detail: ev,
          apply: {
            type: "set_draft",
            draft: appendValueToDraft(draft, maps, ev),
          },
        });
      });
    } else if (def?.value_type === "bool") {
      for (const [label, val] of [
        ["true", "true"],
        ["false", "false"],
      ] as const) {
        items.push({
          id: `enum:${field}:${val}`,
          kind: "enum",
          label,
          apply: {
            type: "set_draft",
            draft: appendValueToDraft(draft, maps, val),
          },
        });
      }
    } else if (def?.unit === "ms") {
      items.push({
        id: "hint:ms",
        kind: "hint",
        label: resolveLabel(
          t,
          "listFilters.valueMs",
          "ms, e.g. 100 or 100ms",
        ),
        apply: { type: "noop" },
      });
    } else if (def?.value_type === "datetime") {
      items.push({
        id: "hint:datetime",
        kind: "hint",
        label: resolveLabel(
          t,
          "listFilters.valueDatetime",
          "e.g. 2026-08-11 12:00:00",
        ),
        apply: { type: "noop" },
      });
    }
    return items;
  }

  return items;
}

function partialOpMatches(op: FilterOp, partial: string): boolean {
  const aliases: Record<string, string[]> = {
    eq: ["=", "==", ":", "eq"],
    neq: ["!=", "<>", "≠", "neq"],
    gt: [">", "gt"],
    gte: [">=", "≥", "gte"],
    lt: ["<", "lt"],
    lte: ["<=", "≤", "lte"],
    contains: ["contains", "包含", "~"],
    starts_with: ["starts_with", "前缀", "^="],
    ends_with: ["ends_with", "后缀"],
    is_null: ["is null", "is_null", "为空", "empty"],
    is_not_null: ["is not null", "is_not_null", "非空", "not empty"],
  };
  return (aliases[op] ?? []).some((a) => a.toLowerCase().startsWith(partial));
}

function buildDraftWithOp(
  draft: string,
  maps: AliasMaps,
  _field: string,
  op: FilterOp,
  opLabel: (op: FilterOp) => string,
): string {
  const fieldMatch = matchFieldRefAtStart(draft, maps);
  const fieldText = fieldMatch
    ? draft.slice(0, fieldMatch.endIndex).trimEnd()
    : draft.trim();
  const token = preferredOpToken(op, opLabel);
  if (op === "is_null" || op === "is_not_null") {
    return `${fieldText} ${token}`;
  }
  return `${fieldText} ${token} `;
}

function preferredOpToken(
  op: FilterOp,
  opLabel: (op: FilterOp) => string,
): string {

  const symbols: Partial<Record<FilterOp, string>> = {
    eq: ":",
    neq: "!=",
    gt: ">",
    gte: ">=",
    lt: "<",
    lte: "<=",
    contains: "contains",
    starts_with: "starts_with",
    ends_with: "ends_with",
    is_null: "is null",
    is_not_null: "is not null",
  };
  return symbols[op] ?? opLabel(op);
}

function appendValueToDraft(
  draft: string,
  maps: AliasMaps,
  value: string,
): string {
  const fieldMatch = matchFieldRefAtStart(draft, maps);
  if (!fieldMatch) return `${draft.trim()} ${value}`;
  const afterField = draft.slice(fieldMatch.endIndex);
  const opMatch = matchOperatorAtStart(afterField);
  if (!opMatch) return `${draft.trim()} ${value}`;
  const prefix = draft
    .slice(0, fieldMatch.endIndex + opMatch.endIndex)
    .trimEnd();
  const encoded = /\s/.test(value) ? `"${value.replace(/"/g, '\\"')}"` : value;
  return `${prefix} ${encoded}`;
}
