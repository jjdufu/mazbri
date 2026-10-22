export interface PaginationConfig {
  current: number;
  size: number;
  total?: number;




  pageCount?: number;
}
export interface ApiResponse<T> {
  code: number;
  message: string;
  data: T;
}
export interface PaginatedResponse<T> {
  data: T[];
  total: number;
  page: number;
  limit: number;
}
export interface RequestOptions {
  signal?: AbortSignal;
}
export type AdminStatus = "active" | "inactive" | "suspended" | "pending";
export type CertificateStatus =
  | "valid"
  | "expiring_soon"
  | "expired"
  | "revoked";
export type ISODateString = string;
export type DecimalString = string;
export type AdminRole = "R_ADMIN" | "R_OPERATOR" | "R_VIEWER" | "R_FINANCE";
export type { AdminUser } from "./admin";
