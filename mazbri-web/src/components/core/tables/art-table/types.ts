import type { TableProps } from "element-plus";
import type { DefaultRow } from "element-plus/es/components/table/src/table/defaults";
import type { PaginationConfig } from "@/types/api/common";
import type { ColumnOption } from "@/types/component";
export type { PaginationConfig } from "@/types/api/common";
export interface PaginationOptions {
  pageSizes?: number[];
  align?: "left" | "center" | "right";




  layout?: string;
  background?: boolean;
  hideOnSinglePage?: boolean;
  size?: "small" | "default" | "large";

  pagerCount?: number;
}
export interface ArtTableProps extends TableProps<DefaultRow> {
  loading?: boolean;
  columns?: ColumnOption[];
  pagination?: PaginationConfig;
  paginationOptions?: PaginationOptions;
  emptyHeight?: string;
  emptyText?: string;
  showTableHeader?: boolean;




  cellCopy?: boolean;
}
export type { ColumnOption } from "@/types/component";
