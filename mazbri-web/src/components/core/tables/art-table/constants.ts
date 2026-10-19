import type { PaginationOptions } from "./types";












export const SIZES_LAYOUT = "sizes";


export const STRIP_LAYOUT_PARTS = new Set([
  "total",
  "pager",
  "jumper",
  "prev",
  "next",
  "slot",
  "->",
]);

export const createDefaultPaginationOptions = (): PaginationOptions => ({
  pageSizes: [10, 20, 30, 50, 100],
  align: "center",
  background: true,
  layout: SIZES_LAYOUT,
  hideOnSinglePage: false,
  size: "default",
});
