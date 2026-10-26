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
export interface MfaVerifyRequest {
  temp_token: string;
  totp_code: string;
}
export interface MfaStatus {
  enabled: boolean;
  has_backup_codes: boolean;
}
export interface MfaEnableRequest {
  totp_code: string;
}
export interface MfaDisableRequest {
  verification_code: string;
}
export interface MfaRequiredSetupRequest {
  temp_token: string;
}
export interface MfaRequiredEnableRequest {
  temp_token: string;
  totp_code: string;
}
