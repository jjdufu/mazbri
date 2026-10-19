import request from "@/utils/http";
import type {
  ConfigEntry,
  ConfigHistory,
  ConfigListQuery,
  ConfigListResponse,
  CreateConfigRequest,
  UpdateConfigRequest,
  ServiceListResponse,
  HistoryListQuery,
  HistoryListResponse,
  RollbackRequest,
  ExportQuery,
  ImportConfigsResult,
  NodeListResponse,
  NodeListQuery,
} from "@/types/api/config-center";
import { API_BASE_PATH, API_PATHS } from "@/config/api";
const BASE_PATH = `${API_BASE_PATH}${API_PATHS.CONFIG_CENTER}`;
export function fetchConfigList(params: ConfigListQuery) {
  return request.get<ConfigListResponse>({
    url: `${BASE_PATH}/configs`,
    params,
  });
}
export function fetchConfig(serviceName: string) {
  return request.get<ConfigEntry>({
    url: `${BASE_PATH}/configs/${serviceName}`,
  });
}
export function createConfig(data: CreateConfigRequest) {
  return request.post<ConfigEntry>({
    url: `${BASE_PATH}/configs`,
    data,
    showSuccessMessage: true,
  });
}
export function updateConfig(
  serviceName: string,
  data: UpdateConfigRequest,
) {
  return request.put<ConfigEntry>({
    url: `${BASE_PATH}/configs/${serviceName}`,
    data,
    showSuccessMessage: true,
  });
}
export function deleteConfig(serviceName: string) {
  return request.del<void>({
    url: `${BASE_PATH}/configs/${serviceName}`,
    showSuccessMessage: true,
  });
}
export function fetchConfigHistory(params: HistoryListQuery) {
  return request.get<HistoryListResponse>({
    url: `${BASE_PATH}/configs/${params.service_name}/history`,
    params: { page: params.page, page_size: params.page_size },
  });
}
export function rollbackConfig(data: RollbackRequest) {
  return request.post<ConfigEntry>({
    url: `${BASE_PATH}/configs/${data.service_name}/rollback/${data.version}`,
    showSuccessMessage: true,
  });
}
export function fetchConfigHistoryVersion(params: {
  service_name: string;
  version_number: number;
}) {
  return request.get<ConfigHistory>({
    url: `${BASE_PATH}/configs/${params.service_name}/history/${params.version_number}`,
  });
}
export function fetchServiceList() {
  return request.get<ServiceListResponse>({
    url: `${BASE_PATH}/services`,
  });
}
export function exportConfigs(params: ExportQuery) {
  const queryParams = new URLSearchParams();
  if (params.service_names?.length) {
    params.service_names.forEach((s) => queryParams.append("service_names", s));
  }
  return request.get<Blob>({
    url: `${BASE_PATH}/export?${queryParams.toString()}`,
    responseType: "blob",
  });
}
export function importConfigs(file: File) {
  const formData = new FormData();
  formData.append("file", file);
  return request.post<ImportConfigsResult>({
    url: `${BASE_PATH}/import`,
    data: formData,
    headers: { "Content-Type": "multipart/form-data" },
  });
}
export interface ValidateTomlResponse {
  valid: boolean;
  error?: {
    line: number;
    column: number;
    message: string;
  };
}
export function validateToml(content: string) {
  return request.post<ValidateTomlResponse>({
    url: `${BASE_PATH}/validate`,
    data: { content },
  });
}
export function fetchNodeList(params?: NodeListQuery) {
  return request.get<NodeListResponse>({
    url: `${BASE_PATH}/nodes`,
    params,
  });
}
