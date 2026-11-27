export * from "./response";
export type Status = 0 | 1;
export type Gender = "male" | "female" | "unknown";
export type SortOrder = "ascending" | "descending";
export type ActionType = "create" | "update" | "delete" | "view";
export type Recordable<T = any> = Record<string, T>;
export type KeyValue<T = any> = {
  key: string;
  value: T;
  label?: string;
};
export interface TimeRange {
  startTime: string;
  endTime: string;
}
export interface FileInfo {
  name: string;
  url: string;
  size: number;
  type: string;
  lastModified?: number;
}
export interface Position {
  x: number;
  y: number;
}
export interface Size {
  width: number;
  height: number;
}
export type Breakpoint = "xs" | "sm" | "md" | "lg" | "xl";
export type ThemeMode = "light" | "dark" | "auto";
export type Language = "zh-CN" | "en-US";
export type Environment = "development" | "production" | "test";
export type DialogType = "add" | "edit";
