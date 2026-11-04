import request from "@/utils/http";
import { API_BASE_PATH } from "@/config/api";
import type {
  DependencyGraph,
  TraceBillingRow,
  TraceDetail,
  TraceSearchQuery,
  TraceSearchResponse,
  TraceServiceCount,
} from "@/types/api/traces";

const BASE_PATH = `${API_BASE_PATH}/stats/traces`;

export function searchTraces(params: TraceSearchQuery) {
  return request.get<TraceSearchResponse>({
    url: `${BASE_PATH}/search`,
    params,
    showErrorMessage: false,
  });
}

export function fetchTrace(
  traceId: string,
  params: { start?: number; end?: number } = {},
) {
  return request.get<TraceDetail>({
    url: `${BASE_PATH}/${traceId}`,
    params,
    showErrorMessage: false,
  });
}

export function fetchTraceDependencies(params: {
  start?: number;
  end?: number;
}) {
  return request.get<DependencyGraph>({
    url: `${BASE_PATH}/dependencies`,
    params,
    showErrorMessage: false,
  });
}

export function fetchTraceBilling(requestIds: string[]) {
  return request.get<TraceBillingRow[]>({
    url: `${BASE_PATH}/billing`,
    params: { request_ids: requestIds.join(",") },
    showErrorMessage: false,
  });
}

export function fetchTraceServices(params: {
  start?: number;
  end?: number;
}) {
  return request.get<TraceServiceCount[]>({
    url: `${BASE_PATH}/services`,
    params,
    showErrorMessage: false,
  });
}
