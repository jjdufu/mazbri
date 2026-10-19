import request from "@/utils/http";
import { API_BASE_PATH } from "@/config/api";
import type { Pagination } from "./route-management";
export interface Supplier {
  id: string;
  supplier_code: string;
  name: string;
  upstream_url: string;
  status: string;
  config?: Record<string, any>;
  created_at: string;
  updated_at: string;
}
export interface SupplierListResponse {
  suppliers: Supplier[];
  pagination: Pagination;
}
export interface CreateSupplierRequest {
  supplier_code: string;
  name: string;
  upstream_url: string;
  config?: Record<string, any>;
}
export interface UpdateSupplierRequest extends Partial<CreateSupplierRequest> {
  status?: string;
  config?: Record<string, any>;
}
export async function fetchSuppliers(params?: {
  page?: number;
  page_size?: number;

  q?: string;
  status?: string;

  filters?: string;
}) {
  return request.get<SupplierListResponse>({
    url: `${API_BASE_PATH}/suppliers`,
    params,
    showErrorMessage: false,
  });
}
export function fetchSupplierById(id: string) {
  return request.get<Supplier>({
    url: `${API_BASE_PATH}/suppliers/${id}`,
  });
}
export function createSupplier(data: CreateSupplierRequest) {
  return request.post<Supplier>({
    url: `${API_BASE_PATH}/suppliers`,
    data,
  });
}
export function updateSupplier(id: string, data: UpdateSupplierRequest) {
  return request.put<Supplier>({
    url: `${API_BASE_PATH}/suppliers/${id}`,
    data,
  });
}
export function deleteSupplier(id: string) {
  return request.del({
    url: `${API_BASE_PATH}/suppliers/${id}`,
  });
}
