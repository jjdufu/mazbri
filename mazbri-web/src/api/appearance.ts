import request from "@/utils/http";
import type {
  AppearanceSettings,
  UpdateAppearanceRequest,
} from "@/types/api/appearance";
import { API_BASE_PATH, API_PATHS } from "@/config/api";
const SETTINGS_PATH = `${API_BASE_PATH}${API_PATHS.SETTINGS}`;
export function fetchAppearanceSettings() {
  return request.get<AppearanceSettings>({
    url: `${SETTINGS_PATH}/appearance`,
  });
}
export function updateAppearanceSettings(data: UpdateAppearanceRequest) {
  return request.put<AppearanceSettings>({
    url: `${SETTINGS_PATH}/appearance`,
    params: data,
  });
}
export function resetAppearanceSettings() {
  return request.post<AppearanceSettings>({
    url: `${SETTINGS_PATH}/appearance/reset`,
  });
}
