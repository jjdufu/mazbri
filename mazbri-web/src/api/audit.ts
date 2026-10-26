import request from "@/utils/http";
import type {
  AuditLog,
  AuditLogListResponse,
  AuditLogListQuery,
  AuditLogDetail,
} from "@/types/api/audit";
import type {
  SystemSettings,
  UpdateSystemSettingsRequest,
  PasswordPolicy,
} from "@/types/api/settings";
import { API_BASE_PATH, API_PATHS } from "@/config/api";
const BASE_PATH = `${API_BASE_PATH}${API_PATHS.AUDIT_LOGS}`;
const SETTINGS_PATH = `${API_BASE_PATH}${API_PATHS.SETTINGS}`;
export function fetchAuditLogList(params?: AuditLogListQuery) {
  return request.get<AuditLogListResponse>({
    url: BASE_PATH,
    params,
  });
}
export function fetchAuditLogById(id: string) {
  return request.get<AuditLogDetail>({
    url: `${BASE_PATH}/${id}`,
  });
}
export function exportAuditLogs(params?: AuditLogListQuery) {
  return request.get<string>({
    url: `${BASE_PATH}/export`,
    params,
  });
}
export function fetchSystemSettings() {
  return request.get<SystemSettings>({
    url: `${SETTINGS_PATH}/system`,
  });
}
export function updateSystemSettings(data: UpdateSystemSettingsRequest) {
  return request.put<SystemSettings>({
    url: `${SETTINGS_PATH}/system`,
    params: data,
  });
}
export function fetchPasswordPolicy() {
  return request.get<PasswordPolicy>({
    url: `${SETTINGS_PATH}/password-policy`,
  });
}
export interface UpdatePasswordPolicyRequest {
  min_length?: number;
  max_length?: number;
  require_uppercase?: boolean;
  require_lowercase?: boolean;
  require_digit?: boolean;
  require_special_char?: boolean;
  special_chars?: string;
  max_age_days?: number;
  warning_days?: number;
  prevent_reuse_count?: number;
  max_login_attempts?: number;
  lockout_duration_minutes?: number;
}
export function updatePasswordPolicy(data: UpdatePasswordPolicyRequest) {
  return request.put<{
    data: PasswordPolicy;
  }>({
    url: `${SETTINGS_PATH}/password-policy`,
    params: data,
    showSuccessMessage: true,
  });
}
export interface MfaSettings {
  totp_issuer: string;
  totp_digits: number;
  totp_period: number;
  backup_codes_count: number;
  updated_at?: string;
}
export interface UpdateMfaSettingsRequest {
  totp_issuer?: string;
  totp_digits?: number;
  totp_period?: number;
  backup_codes_count?: number;
}
export function fetchMfaSettings() {
  return request.get<{
    data: MfaSettings;
  }>({
    url: `${SETTINGS_PATH}/mfa`,
  });
}
export function updateMfaSettings(data: UpdateMfaSettingsRequest) {
  return request.put<{
    data: MfaSettings;
  }>({
    url: `${SETTINGS_PATH}/mfa`,
    params: data,
  });
}
