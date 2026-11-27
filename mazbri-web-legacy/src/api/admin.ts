import request from "@/utils/http";
import type {
  AdminUser,
  AdminListResponse,
  AdminListQuery,
  CreateAdminRequest,
  UpdateAdminRequest,
  ResetPasswordRequest,
  SetStatusRequest,
} from "@/types/api/admin";
import { API_BASE_PATH, API_PATHS } from "@/config/api";

const BASE_PATH = `${API_BASE_PATH}${API_PATHS.ADMINS}`;

export function fetchAdminList(params?: AdminListQuery) {
  return request.get<AdminListResponse>({
    url: BASE_PATH,
    params,
  });
}

export function fetchAdminById(id: string) {
  return request.get<AdminUser>({
    url: `${BASE_PATH}/${id}`,
  });
}

export function createAdmin(
  data: CreateAdminRequest,
  options?: {
    signal?: AbortSignal;
    showSuccessMessage?: boolean;
  },
) {
  return request.post<AdminUser>({
    url: BASE_PATH,
    params: data,
    showSuccessMessage: options?.showSuccessMessage ?? true,
    signal: options?.signal,
  });
}

export function updateAdmin(id: string, data: UpdateAdminRequest) {
  return request.put<AdminUser>({
    url: `${BASE_PATH}/${id}`,
    params: data,
    showSuccessMessage: false,
  });
}

export function deleteAdmin(id: string) {
  return request.del<void>({
    url: `${BASE_PATH}/${id}`,
    showSuccessMessage: false,
  });
}

export function resetAdminPassword(id: string, data: ResetPasswordRequest) {
  return request.put<void>({
    url: `${BASE_PATH}/${id}/password`,
    params: data,
    showSuccessMessage: false,
  });
}

export function setAdminStatus(id: string, data: SetStatusRequest) {
  return request.put<void>({
    url: `${BASE_PATH}/${id}/status`,
    params: data,
    showSuccessMessage: false,
  });
}

export function resetAdminMfa(id: string) {
  return request.post<void>({
    url: `${BASE_PATH}/${id}/reset-mfa`,
    showSuccessMessage: false,
  });
}
