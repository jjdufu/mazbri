import type { AdminRole, AdminStatus, ISODateString } from "./common";
import type {
  ResetPasswordRequest as GeneratedResetPasswordRequest,
  SetAdminStatusRequest as GeneratedSetAdminStatusRequest,
  UpdateAdminRequest as GeneratedUpdateAdminRequest,
} from "./generated/auth";

export type { AdminRole, AdminStatus } from "./common";

export interface AdminUser {
  id: string;
  username: string;
  display_name: string;
  email: string | null;
  role: AdminRole;
  is_active: boolean;
  force_password_reset: boolean;
  mfa_enabled: boolean;
  mfa_required: boolean;
  password_changed_at: ISODateString;
  created_at: ISODateString;
  updated_at: ISODateString;
  last_login_at?: ISODateString | null;
  userName?: string;
  roles?: AdminRole[];
  buttons?: string[];
  isSuperAdmin?: boolean;
  avatar?: string;
}

export type Admin = AdminUser;
export type UserInfo = AdminUser;

export interface AdminListResponse {
  items: Admin[];
  total: number;
  page: number;
  page_size: number;
  total_pages: number;
}

export interface AdminListQuery {
  page?: number;
  page_size?: number;
  q?: string;
  role?: AdminRole;
  status?: AdminStatus;
  filters?: string;
}

export interface CreateAdminRequest {
  username: string;
  display_name: string;
  email?: string;
  password: string;
  role: AdminRole;
  mfa_required?: boolean;
  force_password_reset?: boolean;
}

export interface UpdateAdminRequest extends GeneratedUpdateAdminRequest {
  display_name?: string;
  email?: string;
  role?: AdminRole;
  mfa_required?: boolean;
  force_password_reset?: boolean;
}

export type ResetPasswordRequest = GeneratedResetPasswordRequest;

export type SetStatusRequest = GeneratedSetAdminStatusRequest;
