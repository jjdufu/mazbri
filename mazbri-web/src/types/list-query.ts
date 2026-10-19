




export type FilterOp =
  | "eq"
  | "neq"
  | "contains"
  | "starts_with"
  | "ends_with"
  | "gt"
  | "gte"
  | "lt"
  | "lte"
  | "between"
  | "in"
  | "is_null"
  | "is_not_null";

export type FilterValueType =
  | "string"
  | "enum"
  | "bool"
  | "int"
  | "uint"
  | "decimal"
  | "datetime"
  | "uuid";

export type FilterValue =
  | string
  | number
  | boolean
  | null
  | string[]
  | number[]
  | [string, string]
  | [number, number];

export interface ListFilter {
  field: string;
  op: FilterOp;
  value?: FilterValue;
}

export interface ListQuery {
  page: number;
  page_size: number;
  q?: string;
  filters?: ListFilter[];
  sort?: string;
  order?: "asc" | "desc";
}

export interface FilterFieldDef {
  field: string;
  value_type: FilterValueType;
  ops: FilterOp[];

  in_q?: boolean;

  label_key: string;
  enum_values?: string[];

  enum_label_keys?: string[];
  ui_hidden?: boolean;
  requires_time_window?: boolean;

  unit?: string;
}

export interface QuickFilterDef {
  id: string;
  label_key: string;
  filters: ListFilter[];
}

export interface ResourceFilterCatalog {
  resource: string;
  fields: FilterFieldDef[];
  max_filters?: number;

  require_time_window_when?: string[];
  quick_filters?: QuickFilterDef[];
}

export const DEFAULT_MAX_FILTERS = 16;

export const STRING_OPS: FilterOp[] = [
  "eq",
  "contains",
  "starts_with",
  "ends_with",
  "in",
];
export const ENUM_OPS: FilterOp[] = ["eq", "in", "neq"];
export const BOOL_OPS: FilterOp[] = ["eq"];
export const NUMBER_OPS: FilterOp[] = [
  "eq",
  "neq",
  "gt",
  "gte",
  "lt",
  "lte",
  "between",
  "in",
];
export const DATETIME_OPS: FilterOp[] = [
  "eq",
  "gt",
  "gte",
  "lt",
  "lte",
  "between",
];
export const UUID_OPS: FilterOp[] = ["eq", "in"];
