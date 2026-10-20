import type { ISODateString, CertificateStatus } from "./common";
export type { CertificateStatus } from "./common";
export interface Certificate {
  id: string;
  subject: string;
  dns_names: string[];
  issued_at: ISODateString;
  expires_at: ISODateString;
  revoked: boolean;
  revoked_at: ISODateString | null;
  renewal_count: number;
}
export interface PaginatedCertificateResponse {
  data: Certificate[];
  pagination: {
    page: number;
    limit: number;
    total: number;
  };
}
export type CertificateListResponse = PaginatedCertificateResponse;
export interface FullCertificateResponse {
  id: string;
  subject: string;
  dns_names: string[];
  cert_pem: string;
  key_pem: string;
  ca_cert_pem: string;
  issued_at: ISODateString;
  expires_at: ISODateString;
  status: CertificateStatus;
}
export interface IssueCertificateRequest {
  subject: string;
  validity_days?: number;
}
export interface IssueCertificateResponse {
  id: string;
  subject: string;
  dns_names: string[];
  issued_at: ISODateString;
  expires_at: ISODateString;
  cert_pem: string;
  key_pem: string;
  ca_cert_pem: string;
}
export interface RenewCertificateRequest {
  validity_days?: number;
}
export interface AutoRenewalConfig {
  enabled: boolean;
  threshold_days: number;
  validity_days: number;
}
export interface AutoRenewalConfigResponse {
  certificate_id: string;
  enabled: boolean;
  threshold_days: number;
  validity_days: number;
  last_renewal_at: ISODateString | null;
  next_renewal_at: ISODateString | null;
  certificate_expires_at: ISODateString;
}
