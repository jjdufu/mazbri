import request from "@/utils/http";
import type {
  AdminUser,
  LoginResponse,
  FirstLoginChangePasswordRequest,
  ChangePasswordRequest,
  MfaSetupResponse,
  MfaVerifyRequest,
  MfaStatus,
  MfaBackupCodesResponse,
  MfaSessionRefresh,
  MfaStepUpRequest,
  MfaStepUpResponse,
  MfaStepUpStatus,
} from "@/types/api";
import { API_BASE_PATH, API_PATHS } from "@/config/api";
const AUTH_PATH = `${API_BASE_PATH}${API_PATHS.AUTH}`;
export { refreshAccessToken as refreshAdminSession } from "@/utils/http";
export interface LoginParams {
  username: string;
  password: string;
}
export function fetchLogin(params: LoginParams, signal?: AbortSignal) {
  return request.post<LoginResponse>({
    url: `${AUTH_PATH}/login`,
    params,
    signal,
  });
}
export const login = fetchLogin;
export function fetchGetUserInfo(signal?: AbortSignal): Promise<AdminUser> {
  return request.get<AdminUser>({
    url: `${AUTH_PATH}/me`,
    signal,
  });
}
export const getCurrentUser = fetchGetUserInfo;
export function firstLoginChangePassword(
  data: FirstLoginChangePasswordRequest,
  tempToken?: string,
  signal?: AbortSignal,
): Promise<LoginResponse> {
  return request.post<LoginResponse>({
    url: `${AUTH_PATH}/first-login-change-password`,
    params: data,
    showSuccessMessage: true,
    signal,
    headers: tempToken ? { Authorization: `Bearer ${tempToken}` } : undefined,
  });
}
/** 已登录用户自助修改密码：后端为 `PUT /auth/change-password`，需携带确认密码。 */
export function changePassword(
  data: ChangePasswordRequest,
  signal?: AbortSignal,
) {
  return request.put<void>({
    url: `${AUTH_PATH}/change-password`,
    params: data,
    showSuccessMessage: true,
    signal,
  });
}
export function logout(
  _data?: { refresh_token?: string },
  signal?: AbortSignal,
) {
  return request.post<void>({
    url: `${AUTH_PATH}/logout`,
    data: {},
    signal,
    showErrorMessage: false,
  });
}
export function getMfaSetup(currentPassword: string, signal?: AbortSignal) {
  return request.post<MfaSetupResponse>({
    url: `${AUTH_PATH}/mfa/setup`,
    data: { current_password: currentPassword },
    signal,
  });
}
export function enableMfa(
  data: {
    totp_code: string;
  },
  signal?: AbortSignal,
) {
  return request.post<MfaSessionRefresh>({
    url: `${AUTH_PATH}/mfa/enable`,
    data,
    showSuccessMessage: true,
    signal,
  });
}
export function disableMfa(signal?: AbortSignal) {
  return request.post<MfaSessionRefresh>({
    url: `${AUTH_PATH}/mfa/disable`,
    showSuccessMessage: true,
    signal,
  });
}
function exclusiveMfaBody(data: {
  totp_code?: string;
  backup_code?: string;
}): { totp_code: string } | { backup_code: string } {
  const totp = data.totp_code?.trim() ?? "";
  const backup = data.backup_code?.trim().toUpperCase() ?? "";
  if ((totp.length > 0) === (backup.length > 0)) {
    throw new Error("MFA code must be exactly one of totp_code or backup_code");
  }
  return totp ? { totp_code: totp } : { backup_code: backup };
}
export function verifyMfa(data: MfaVerifyRequest, signal?: AbortSignal) {
  return request.post<LoginResponse>({
    url: `${AUTH_PATH}/mfa/verify`,
    data: {
      temp_token: data.temp_token,
      ...exclusiveMfaBody(data),
    },
    signal,
  });
}
export function getMfaStepUp(signal?: AbortSignal) {
  return request.get<MfaStepUpStatus>({
    url: `${AUTH_PATH}/mfa/step-up`,
    signal,
    showErrorMessage: false,
  });
}
export function stepUpMfa(data: MfaStepUpRequest, signal?: AbortSignal) {
  return request.post<MfaStepUpResponse>({
    url: `${AUTH_PATH}/mfa/step-up`,
    data: {
      ...exclusiveMfaBody(data),
      ...(data.purpose ? { purpose: data.purpose } : {}),
    },
    signal,
    showErrorMessage: false,
  });
}
export function endMfaStepUp(signal?: AbortSignal) {
  return request.del<void>({
    url: `${AUTH_PATH}/mfa/step-up`,
    signal,
    showErrorMessage: false,
  });
}
export function getMfaStatus(signal?: AbortSignal) {
  return request.get<MfaStatus>({
    url: `${AUTH_PATH}/mfa/status`,
    signal,
  });
}
/** 重新生成备用码（要求已启用 MFA）；旧备用码立即作废。 */
export function regenerateBackupCodes(signal?: AbortSignal) {
  return request.post<MfaBackupCodesResponse>({
    url: `${AUTH_PATH}/mfa/regenerate-codes`,
    signal,
  });
}

export function mfaRequiredSetup(
  data: {
    temp_token: string;
  },
  signal?: AbortSignal,
) {
  return request.post<MfaSetupResponse>({
    url: `${AUTH_PATH}/mfa/required-setup`,
    params: data,
    signal,
  });
}
export function mfaRequiredEnable(
  data: {
    temp_token: string;
    totp_code: string;
  },
  signal?: AbortSignal,
) {
  return request.post<LoginResponse>({
    url: `${AUTH_PATH}/mfa/required-enable`,
    params: data,
    showSuccessMessage: true,
    signal,
  });
}

