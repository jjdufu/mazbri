import request from "@/utils/http";
import { API_BASE_PATH } from "@/config/api";
import type {
  MetricOverview,
  MetricSeries,
  MetricTimeseriesQuery,
  MetricsCatalog,
} from "@/types/api/monitoring";
const BASE_PATH = `${API_BASE_PATH}/stats/metrics`;
export async function fetchMetricsOverview() {
  return request.get<MetricOverview>({
    url: `${BASE_PATH}/overview`,
    showErrorMessage: false,
  });
}
export async function fetchMetricsCatalog() {
  return request.get<MetricsCatalog>({
    url: `${BASE_PATH}/catalog`,
    showErrorMessage: false,
  });
}
export async function fetchMetricTimeseries(params: MetricTimeseriesQuery) {
  return request.get<MetricSeries>({
    url: `${BASE_PATH}/timeseries`,
    params,
    showErrorMessage: false,
  });
}
