export interface SystemSettings {
  [key: string]: any;
  site_name?: string;
  site_url?: string;
  tenant_gateway_url?: string;
  session_timeout?: number;
  max_upload_size?: number;
  enable_audit_log?: boolean;
  watermark_visible?: boolean;
}
export interface UpdateSystemSettingsRequest {
  site_name?: string;
  site_url?: string;
  tenant_gateway_url?: string;
  session_timeout_minutes?: number;
  session_timeout?: number;
  max_upload_size_mb?: number;
  max_upload_size?: number;
  enable_audit_log?: boolean;
  watermark_visible?: boolean;
}

export interface UserSettings {
  language: string;
  email?: string;
}
export interface UpdateUserSettingsRequest {
  language?: string;
  email?: string;
}
export interface PasswordPolicy {
  min_length: number;
  max_length: number;
  require_uppercase: boolean;
  require_lowercase: boolean;
  require_digit: boolean;
  require_special_char: boolean;
  special_chars: string;
  max_age_days: number;
  warning_days: number;
  prevent_reuse_count: number;
  max_login_attempts: number;
  lockout_duration_minutes: number;
  updated_at?: string;
}
