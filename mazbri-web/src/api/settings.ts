import request from "@/utils/http";
import type {
  SystemSettings,
  UpdateSystemSettingsRequest,
  UserSettings,
  UpdateUserSettingsRequest,
  PasswordPolicy,
} from "@/types/api";
import { API_BASE_PATH, API_PATHS } from "@/config/api";
const BASE_PATH = `${API_BASE_PATH}${API_PATHS.SETTINGS}`;
export function fetchSystemSettings() {
  return request.get<SystemSettings>({
    url: `${BASE_PATH}/system`,
  });
}
export const getSystemSettings = fetchSystemSettings;
export function updateSystemSettings(data: UpdateSystemSettingsRequest) {
  return request.put<SystemSettings>({
    url: `${BASE_PATH}/system`,
    params: data,
  });
}
export function getUserSettings() {
  return request.get<UserSettings>({
    url: `${BASE_PATH}/user`,
  });
}
export function updateUserSettings(data: UpdateUserSettingsRequest) {
  return request.put<UserSettings>({
    url: `${BASE_PATH}/user`,
    params: data,
  });
}
export function fetchPasswordPolicy() {
  return request.get<PasswordPolicy>({
    url: `${BASE_PATH}/password-policy`,
  });
}
export const getPasswordPolicy = fetchPasswordPolicy;
