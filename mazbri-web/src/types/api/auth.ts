export type { AdminUser, Admin, UserInfo } from "./admin";
export interface LoginRequest {
  username: string;
  password: string;
}
export interface LoginResponse {
  token?: string;
  expires_in?: number;
  refresh_token?: string;

  cookie_auth?: boolean;
  user?: import("./admin").AdminUser;
  mfa_required?: boolean;
  mfa_setup_required?: boolean;
  temp_token?: string;
  password_reset_required?: boolean;
  role?: string;
  username?: string;
  message?: string;
  totp_digits?: number;
}
export interface FirstLoginChangePasswordRequest {
  username: string;
  current_password: string;
  new_password: string;
  new_username?: string;
}
export interface ChangePasswordRequest {
  current_password: string;
  new_password: string;
  confirm_password: string;
}
export interface MfaSetupResponse {
  secret: string;
  qr_code_url: string;
  backup_codes: string[];
  totp_digits?: number;
  /** 登录强制设置 MFA 流程中轮换出的新 mfa_setup 临时令牌 */
  temp_token?: string;
}
/** 提交时 totp_code 与 backup_code 有且只有一个。 */
export interface MfaVerifyRequest {
  temp_token: string;
  totp_code?: string;
  backup_code?: string;
}
/** `GET /auth/mfa/status`：当前登录账号自己的 MFA 状态。 */
export interface MfaStatus {
  enabled: boolean;
  changed_at: string | null;
  backup_codes_remaining: number;
  mfa_required: boolean;
  totp_digits: number;
}
/** `POST /auth/mfa/regenerate-codes` 的响应：新的备用码（旧备用码立即作废）。 */
export interface MfaBackupCodesResponse {
  backup_codes: string[];
}
export interface MfaEnableRequest {
  totp_code: string;
}
/** 启用或关闭后重签会话时，响应里用于更新本地会话到期的字段。 */
export interface MfaSessionRefresh {
  expires_at?: string;
  remaining_secs?: number;
  expires_in?: number;
  token?: string;
  cookie_auth?: boolean;
}
export interface MfaStepUpStatus {
  mfa_enabled: boolean;
  mfa_required: boolean;
  totp_digits: number;
  active: boolean;
  expires_at?: string | null;
  remaining_secs?: number;
  ttl_secs?: number;
  server_time?: string | null;
  reason?: string | null;
}
/** totp_code 与 backup_code 有且只有一个，purpose 可选。 */
export interface MfaStepUpRequest {
  totp_code?: string;
  backup_code?: string;
  purpose?: string;
}
export interface MfaStepUpResponse {
  active: boolean;
  opened_at?: string;
  expires_at?: string;
  remaining_secs: number;
  ttl_secs: number;
  server_time?: string;
  method?: string;
  totp_digits?: number;
  fresh?: {
    purpose: string;
    expires_in_secs: number;
  };
}
export interface MfaRequiredSetupRequest {
  temp_token: string;
}
export interface MfaRequiredEnableRequest {
  temp_token: string;
  totp_code: string;
}
