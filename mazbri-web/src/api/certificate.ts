import request from "@/utils/http";
import type {
  AutoRenewalConfig,
  AutoRenewalConfigResponse,
  CertificateListResponse,
  FullCertificateResponse,
  IssueCertificateRequest,
  IssueCertificateResponse,
  RenewCertificateRequest,
} from "@/types/api/certificate";
import { API_BASE_PATH, API_PATHS } from "@/config/api";

const BASE_PATH = `${API_BASE_PATH}${API_PATHS.CERTIFICATES}`;

export function fetchCertificateList(params?: {
  page?: number;
  limit?: number;
  status?: string;
  offset?: number;

  q?: string;
}) {
  return request.get<CertificateListResponse>({
    url: BASE_PATH,
    params,
  });
}

export function fetchCertificateById(id: string) {
  return request.get<FullCertificateResponse>({
    url: `${BASE_PATH}/${id}/full`,
  });
}

export function issueCertificate(
  data: IssueCertificateRequest,
  options?: {
    signal?: AbortSignal;
    showSuccessMessage?: boolean;
  },
) {
  return request.post<IssueCertificateResponse>({
    url: `${BASE_PATH}/issue`,
    params: data,
    showSuccessMessage: options?.showSuccessMessage ?? true,
    signal: options?.signal,
  });
}

export function renewCertificate(id: string, data?: RenewCertificateRequest) {
  return request.post<IssueCertificateResponse>({
    url: `${BASE_PATH}/renew/${id}`,
    params: data,
    showSuccessMessage: true,
  });
}

export function revokeCertificate(id: string) {
  return request.post<void>({
    url: `${BASE_PATH}/revoke/${id}`,
    showSuccessMessage: true,
  });
}

export function downloadCertificate(id: string) {
  return request.get<{
    cert_pem: string;
    key_pem: string;
    ca_pem?: string;
  }>({
    url: `${BASE_PATH}/${id}/download`,
  });
}

export function updateAutoRenewal(id: string, config: AutoRenewalConfig) {
  return request.put<void>({
    url: `${BASE_PATH}/${id}/auto-renewal`,
    params: config,
    showSuccessMessage: true,
  });
}

export function getAutoRenewalConfig(id: string) {
  return request.get<AutoRenewalConfigResponse>({
    url: `${BASE_PATH}/${id}/auto-renewal`,
  });
}
