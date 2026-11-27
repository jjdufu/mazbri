export interface ColumnOption<T = Record<string, unknown>> {
  prop?: string;
  label?: string;
  width?: number | string;
  minWidth?: number | string;
  align?: "left" | "center" | "right";
  headerAlign?: "left" | "center" | "right";
  className?: string;
  fixed?: boolean | "left" | "right";
  sortable?: boolean | "custom";
  visible?: boolean;
  formatter?: (row: T) => unknown;
}
