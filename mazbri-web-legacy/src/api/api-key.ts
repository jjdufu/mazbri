import request from "@/utils/http";
import type {
  ApiKey,
  ApiKeyListResponse,
  CreateApiKeyRequest,
  CreateApiKeyResponse,
  UpdateApiKeyRequest,
  UpdateApiKeyResponse,
} from "@/types/api";
import { API_BASE_PATH } from "@/config/api";
export function getApiKeyList(tenantId: string) {
  return request.get<ApiKeyListResponse>({
    url: `${API_BASE_PATH}/tenants/${tenantId}/api-keys`,
  });
}
export function createApiKey(tenantId: string, data: CreateApiKeyRequest) {
  return request.post<CreateApiKeyResponse>({
    url: `${API_BASE_PATH}/tenants/${tenantId}/api-keys`,
    params: data,
  });
}
export function updateApiKey(
  tenantId: string,
  keyPrefix: string,
  data: UpdateApiKeyRequest,
) {
  return request.put<UpdateApiKeyResponse>({
    url: `${API_BASE_PATH}/tenants/${tenantId}/api-keys/${keyPrefix}`,
    params: data,
  });
}
export function revokeApiKey(tenantId: string, keyPrefix: string) {
  return request.del<void>({
    url: `${API_BASE_PATH}/tenants/${tenantId}/api-keys/${keyPrefix}`,
  });
}
