import request from "@/utils/http";
import type {
  Service,
  CreateServiceRequest,
  UpdateServiceRequest,
  ServiceValidationResponse,
  ServiceNameQuery,
} from "@/types/api/service-registry";
import { API_BASE_PATH } from "@/config/api";
const BASE_PATH = `${API_BASE_PATH}/service-names`;
export function fetchServiceList(params?: ServiceNameQuery) {
  return request.get<Service[]>({
    url: BASE_PATH,
    params,
  });
}
export function fetchService(serviceName: string) {
  return request.get<Service>({
    url: `${BASE_PATH}/${serviceName}`,
  });
}
export function createService(data: CreateServiceRequest) {
  return request.post<Service>({
    url: BASE_PATH,
    data,
    showSuccessMessage: true,
  });
}
export function updateService(serviceName: string, data: UpdateServiceRequest) {
  return request.put<Service>({
    url: `${BASE_PATH}/${serviceName}`,
    data,
    showSuccessMessage: true,
  });
}
export function deleteService(serviceName: string) {
  return request.del<void>({
    url: `${BASE_PATH}/${serviceName}`,
    showSuccessMessage: true,
  });
}
export function validateService(serviceName: string) {
  return request.get<ServiceValidationResponse>({
    url: `${BASE_PATH}/${serviceName}/validate`,
  });
}
