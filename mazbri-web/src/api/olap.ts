import request from "@/utils/http";
import { API_BASE_PATH } from "@/config/api";
import type { OlapResponse } from "@/types/api/olap";

const PATH = `${API_BASE_PATH}/stats/olap/query`;

export async function postOlapQuery<T = Record<string, unknown>>(body: {
  template: string;
  params: Record<string, unknown>;
}) {
  return request.post<OlapResponse<T>>({
    url: PATH,
    data: body,
    showErrorMessage: false,
  });
}
