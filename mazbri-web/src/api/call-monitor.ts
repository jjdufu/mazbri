import request from "@/utils/http";
import { API_BASE_PATH } from "@/config/api";
import type {
  CallHistogramQuery,
  CallHistogramResponse,
  CallMonitorMode,
  CallStageQuery,
  CallStageResponse,
  CallMonitorQuery,
  CallMonitorResponse,
} from "@/types/api/call-monitor";
const BASE_PATH = `${API_BASE_PATH}/stats/calls`;
export async function fetchCustomerCallLogs(params: CallMonitorQuery) {
  return request.get<CallMonitorResponse>({
    url: `${BASE_PATH}/customer`,
    params,
    showErrorMessage: false,
  });
}
export async function fetchUpstreamCallLogs(params: CallMonitorQuery) {
  return request.get<CallMonitorResponse>({
    url: `${BASE_PATH}/upstream`,
    params,
    showErrorMessage: false,
  });
}
export async function fetchCallHistogram(
  mode: CallMonitorMode,
  params: CallHistogramQuery,
) {
  return request.get<CallHistogramResponse>({
    url: `${BASE_PATH}/${mode}/histogram`,
    params,
    showErrorMessage: false,
  });
}
export async function fetchCallStages(params: CallStageQuery) {
  return request.get<CallStageResponse>({
    url: `${BASE_PATH}/stages`,
    params,
    showErrorMessage: false,
  });
}
