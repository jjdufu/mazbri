import request from "@/utils/http";
import type { LicenseStatus } from "@/types/api/authorizations";
import { API_BASE_PATH, API_PATHS } from "@/config/api";

const BASE_PATH = `${API_BASE_PATH}${API_PATHS.CERTIFICATES}/authorizations`;

export function fetchLicenseStatus(signal?: AbortSignal) {
  return request.get<LicenseStatus>({
    url: `${BASE_PATH}/license/status`,
    signal,
  });
}

export function exportLicenseRequest(signal?: AbortSignal) {
  return request.post<{
    version: number;
    installationId: string;
    publicKey: string;
    currentSequence: number;
    keyVersion: string | null;
    customerId: string | null;
    productVersion: string;
  }>({
    url: `${BASE_PATH}/license/request`,
    signal,
  });
}

export function importLicense(licenseFile: File, signal?: AbortSignal) {
  const formData = new FormData();
  formData.append("license_file", licenseFile);
  return request.post<{
    status: string;
    issuanceAllowed: boolean;
    sequence: number | null;
  }>({
    url: `${BASE_PATH}/license/import`,
    data: formData,
    signal,
    showSuccessMessage: true,
  });
}
