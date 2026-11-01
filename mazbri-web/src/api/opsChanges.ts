import request from "@/utils/http";
import { API_BASE_PATH } from "@/config/api";

export interface ChangeEvent {
  ts: string;
  kind: string;
  title: string;
  actor?: string | null;
  resource_type?: string | null;
  resource_id?: string | null;
  status?: string | null;
  service?: string | null;
  detail?: Record<string, unknown>;
}

export interface ChangesResponse {
  events: ChangeEvent[];
}

export async function fetchOpsChanges(params: {
  start?: number;
  end?: number;
  limit?: number;
}) {
  return request.get<ChangesResponse>({
    url: `${API_BASE_PATH}/stats/ops/changes`,
    params,
    showErrorMessage: false,
  });
}
