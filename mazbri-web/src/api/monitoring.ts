import request from "@/utils/http";
import { API_BASE_PATH } from "@/config/api";
import type {
  MetricOverview,
  MetricSeries,
  MetricTimeseriesQuery,
  MetricsCatalog,
  ServiceHealthRow,
  TopologyGraph,
} from "@/types/api/monitoring";
const BASE_PATH = `${API_BASE_PATH}/stats/metrics`;
export type MetricsOverviewQuery = {
  start?: number;
  end?: number;
  /** `instant` = 此刻近瞬时；缺省且带 start/end = 跟时间窗 */
  mode?: "instant" | "range";
};

/** Overview KPI：默认跟时间窗；`mode: "instant"` 为此刻。曲线请用 fetchMetricTimeseries。 */
export async function fetchMetricsOverview(params: MetricsOverviewQuery = {}) {
  return request.get<MetricOverview>({
    url: `${BASE_PATH}/overview`,
    params,
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
export async function fetchServicesHealth() {
  return request.get<ServiceHealthRow[]>({
    url: `${BASE_PATH}/services/health`,
    showErrorMessage: false,
  });
}

export async function fetchServiceTopology(params?: { start?: number; end?: number }) {
  return request.get<TopologyGraph>({
    url: `${BASE_PATH}/topology`,
    params,
    showErrorMessage: false,
  });
}
