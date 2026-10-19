import request from "@/utils/http";
import type {
  Product,
  ProductListResponse,
  ProductListQuery,
  CreateProductRequest,
  UpdateProductRequest,
  ProductTenantListResponse,
  ProductIdReservation,
  AuthorizationListQuery,
  AuthorizationListResponse,
} from "@/types/api/product";
import { API_BASE_PATH, API_PATHS } from "@/config/api";
import { normalizeUpstreamUrlInput } from "@/utils";
const BASE_PATH = `${API_BASE_PATH}${API_PATHS.PRODUCTS}`;
const AUTH_BASE_PATH = `${API_BASE_PATH}${API_PATHS.AUTHORIZATIONS}`;
function normalizeProductListQuery(params?: ProductListQuery) {
  if (!params) return undefined;
  const { limit, page_size, ...rest } = params;
  return {
    ...rest,
    ...(typeof page_size === "number"
      ? { page_size }
      : typeof limit === "number"
        ? { page_size: limit }
        : {}),
  };
}
export function fetchProductList(params?: ProductListQuery) {
  return request.get<ProductListResponse>({
    url: BASE_PATH,
    params: normalizeProductListQuery(params),
  });
}
export function fetchProductById(id: string) {
  return request.get<Product>({
    url: `${BASE_PATH}/${id}`,
  });
}
export function createProduct(data: CreateProductRequest) {
  const payload = {
    ...data,
  };
  return request.post<Product>({
    url: BASE_PATH,
    params: payload,
    showSuccessMessage: true,
  });
}
export function createProductReservation() {
  return request.post<ProductIdReservation>({
    url: `${BASE_PATH}/reservations`,
  });
}
export function releaseProductReservation(id: string) {
  return request.del<{
    released: boolean;
  }>({
    url: `${BASE_PATH}/reservations/${id}`,
  });
}
export function updateProduct(id: string, data: UpdateProductRequest) {
  const payload: UpdateProductRequest = {
    ...data,
  };
  return request.put<Product>({
    url: `${BASE_PATH}/${id}`,
    params: payload,
    showSuccessMessage: true,
  });
}
export function deleteProduct(id: string) {
  return request.del<void>({
    url: `${BASE_PATH}/${id}`,
    showSuccessMessage: true,
  });
}
export function fetchProductTenants(id: string) {
  return request.get<ProductTenantListResponse>({
    url: `${BASE_PATH}/${id}/tenants`,
  });
}
export function fetchAuthorizations(params?: AuthorizationListQuery) {
  return request.get<AuthorizationListResponse>({
    url: AUTH_BASE_PATH,
    params,
  });
}
