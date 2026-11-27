import request from "@/utils/http";
import type {
  AutoRenewalConfig,
  AutoRenewalConfigResponse,
  CaCertificateResponse,
  CertificateListResponse,
  FullCertificateResponse,
  IssueCertificateRequest,
  IssueCertificateResponse,
  PublicCertificateDetailResponse,
  RenewCertificateRequest,
} from "@/types/api/certificate";
import { API_BASE_PATH, API_PATHS } from "@/config/api";

const BASE_PATH = `${API_BASE_PATH}${API_PATHS.CERTIFICATES}`;

let cachedCaCertPromise: Promise<CaCertificateResponse> | null = null;

export function fetchCertificateList(params?: {
  page?: number;
  limit?: number;
  status?: string;
  offset?: number;
  q?: string;
  filters?: string;
  include_revoked?: boolean;
}) {
  return request.get<CertificateListResponse>({
    url: BASE_PATH,
    params,
  });
}

export function resetCaCertificateCache(): void {
  cachedCaCertPromise = null;
}

export function fetchCertificateDetail(id: string, signal?: AbortSignal) {
  return request.get<PublicCertificateDetailResponse>({
    url: `${BASE_PATH}/${id}`,
    signal,
  });
}

export function fetchIssuedCertificateSubjects() {
  return request.get<string[]>({
    url: `${BASE_PATH}/subjects`,
  });
}

export function fetchCaCertificate(force = false): Promise<CaCertificateResponse> {
  if (!force && cachedCaCertPromise) {
    return cachedCaCertPromise;
  }
  cachedCaCertPromise = request
    .get<CaCertificateResponse>({
      url: `${BASE_PATH}/ca`,
    })
    .catch((error) => {
      cachedCaCertPromise = null;
      throw error;
    });
  return cachedCaCertPromise;
}

export function fetchCertificateFullWithPrivateKey(
  id: string,
  signal?: AbortSignal,
) {
  return request.get<FullCertificateResponse>({
    url: `${BASE_PATH}/${id}/full`,
    signal,
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

export function updateAutoRenewal(id: string, config: AutoRenewalConfig) {
  return request.put<void>({
    url: `${BASE_PATH}/${id}/auto-renewal`,
    params: config,
    showSuccessMessage: true,
  });
}

export function getAutoRenewalConfig(id: string, signal?: AbortSignal) {
  return request.get<AutoRenewalConfigResponse>({
    url: `${BASE_PATH}/${id}/auto-renewal`,
    signal,
  });
}
