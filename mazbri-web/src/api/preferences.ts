import request from "@/utils/http";
import { API_BASE_PATH } from "@/config/api";

export interface UserPreferences {
  last_path: string;
}

export function getPreferences() {
  return request.get<UserPreferences>({
    url: `${API_BASE_PATH}/admin/preferences`,
    showErrorMessage: false,
  });
}

/** 个人 UI 偏好（非安全审计）。失败静默，禁止弹「安全审计」相关文案。 */
export function putPreferences(body: Partial<UserPreferences>) {
  return request.put<UserPreferences>({
    url: `${API_BASE_PATH}/admin/preferences`,
    data: body,
    showErrorMessage: false,
  });
}
