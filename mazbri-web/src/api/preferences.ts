import request from "@/utils/http";
import { API_BASE_PATH } from "@/config/api";

export interface UserPreferences {
  home_path: string;
  last_path: string;
  dashboards: Array<{
    id: string;
    title: string;
    widgets: Array<{ template: string; params: Record<string, unknown>; type?: string }>;
  }>;
}

export function getPreferences() {
  return request.get<UserPreferences>({ url: `${API_BASE_PATH}/admin/preferences` });
}

export function putPreferences(body: Partial<UserPreferences>) {
  return request.put<UserPreferences>({
    url: `${API_BASE_PATH}/admin/preferences`,
    data: body,
  });
}
