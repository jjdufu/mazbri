export interface SystemSettings {
  [key: string]: any;
  site_name?: string;
  site_url?: string;

  tenant_gateway_url?: string;
  admin_email?: string;
  session_timeout?: number;
  max_upload_size?: number;
  enable_audit_log?: boolean;
}
export interface UpdateSystemSettingsRequest {
  site_name?: string;
  site_url?: string;
  tenant_gateway_url?: string;
  admin_email?: string;
  default_tenant_plan?: string;
  default_tenant_rate_limit?: number;
  default_tenant_cache_ttl?: number;
  session_timeout_minutes?: number;
  max_login_attempts?: number;
  session_timeout?: number;
  enable_audit_log?: boolean;
  max_upload_size?: number;
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
