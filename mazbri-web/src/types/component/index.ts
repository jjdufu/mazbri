import type { VNode } from "vue";
import type { FormItemRule } from "element-plus";
export type SearchComponentType =
  | "input"
  | "select"
  | "radio"
  | "checkbox"
  | "date"
  | "datetime"
  | "daterange"
  | "datetimerange"
  | "month"
  | "monthrange"
  | "year"
  | "yearrange"
  | "week"
  | "time"
  | "timerange";
export interface SearchChangeParams {
  prop: string;
  val: unknown;
}
export interface ColumnOption<T = unknown> {
  type?: "selection" | "expand" | "index" | "globalIndex";
  prop?: string;
  label?: string;
  width?: string | number;
  minWidth?: string | number;
  fixed?: boolean | "left" | "right";
  sortable?: boolean;
  filters?: {
    text: string;
    value: string | number;
  }[];
  filterMethod?(
    value: string | number,
    row: T,
    column: ColumnOption<T>,
  ): boolean;
  filterPlacement?: string;
  disabled?: boolean;
  visible?: boolean;
  checked?: boolean;
  formatter?(row: T): string | number | VNode | undefined;
  useSlot?: boolean;
  slotName?: string;
  useHeaderSlot?: boolean;
  headerSlotName?: string;
  [key: string]: unknown;
}
export type { PaginationConfig } from "@/types/api/common";
export interface PaginationOptions {
  total: number;
  pageSizes?: number[];
  layout?: string;
  small?: boolean;
}
export interface FormRule {
  required?: boolean;
  message?: string;
  trigger?: string | string[];
  min?: number;
  max?: number;
  pattern?: RegExp;
  validator?: (
    rule: FormItemRule,
    value: unknown,
    callback: (error?: string | Error) => void,
  ) => void;
}
export interface DialogConfig {
  title: string;
  visible: boolean;
  width?: string | number;
  closeOnClickModal?: boolean;
  closeOnPressEscape?: boolean;
  showClose?: boolean;
  lockScroll?: boolean;
  modal?: boolean;
  customClass?: string;
}
