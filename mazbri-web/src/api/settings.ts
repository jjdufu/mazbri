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
const SYSTEM_SETTINGS_TTL_MS = 10_000;
let systemSettingsCache: { data: SystemSettings; at: number } | null = null;
let systemSettingsInflight: Promise<SystemSettings> | null = null;

export function fetchSystemSettings(force = false): Promise<SystemSettings> {
  if (!force && systemSettingsInflight) return systemSettingsInflight;
  if (
    !force &&
    systemSettingsCache &&
    Date.now() - systemSettingsCache.at < SYSTEM_SETTINGS_TTL_MS
  ) {
    return Promise.resolve(systemSettingsCache.data);
  }
  systemSettingsInflight = request
    .get<SystemSettings>({
      url: `${BASE_PATH}/system`,
    })
    .then((data) => {
      systemSettingsCache = { data, at: Date.now() };
      return data;
    })
    .finally(() => {
      systemSettingsInflight = null;
    });
  return systemSettingsInflight;
}
export const getSystemSettings = fetchSystemSettings;
export function updateSystemSettings(data: UpdateSystemSettingsRequest) {
  return request
    .put<SystemSettings>({
      url: `${BASE_PATH}/system`,
      params: data,
    })
    .then((res) => {
      systemSettingsCache = { data: res, at: Date.now() };
      return res;
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
