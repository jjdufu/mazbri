import request from "@/utils/http";
import { API_BASE_PATH } from "@/config/api";
import type {
  AlertEventPage,
  AlertEventQuery,
  AlertRule,
  AlertRuleInput,
  AlertSummary,
} from "@/types/api/alerts";

const BASE_PATH = `${API_BASE_PATH}/alerts`;

export function fetchAlertRules() {
  return request.get<AlertRule[]>({ url: `${BASE_PATH}/rules` });
}

export function createAlertRule(data: AlertRuleInput) {
  return request.post<AlertRule>({ url: `${BASE_PATH}/rules`, data });
}

export function updateAlertRule(id: string, data: AlertRuleInput) {
  return request.put<AlertRule>({ url: `${BASE_PATH}/rules/${id}`, data });
}

export function setAlertRuleEnabled(id: string, enabled: boolean) {
  return request.put<AlertRule>({
    url: `${BASE_PATH}/rules/${id}/enabled`,
    data: { enabled },
  });
}

export function deleteAlertRule(id: string) {
  return request.del<{ id: string }>({ url: `${BASE_PATH}/rules/${id}` });
}

export function fetchAlertEvents(params: AlertEventQuery) {
  return request.get<AlertEventPage>({ url: `${BASE_PATH}/events`, params });
}

export function fetchAlertSummary() {
  return request.get<AlertSummary>({
    url: `${BASE_PATH}/summary`,
    showErrorMessage: false,
  });
}

export interface AlertSilence {
  id: string;
  matchers: Record<string, string>;
  starts_at: string;
  ends_at: string;
  created_by: string;
  comment: string;
  created_at: string;
}

export interface AlertInhibit {
  id: string;
  name: string;
  source_matchers: Record<string, string>;
  target_matchers: Record<string, string>;
  equal_labels: string[];
  enabled: boolean;
  created_at: string;
  updated_at: string;
}



export function fetchAlertSilences() {
  return request.get<AlertSilence[]>({ url: `${BASE_PATH}/silences` });
}
export function createAlertSilence(data: {
  matchers: Record<string, string>;
  starts_at: string;
  ends_at: string;
  comment?: string;
}) {
  return request.post<AlertSilence>({ url: `${BASE_PATH}/silences`, data });
}
export function deleteAlertSilence(id: string) {
  return request.del<{ id: string }>({ url: `${BASE_PATH}/silences/${id}` });
}

export function fetchAlertInhibits() {
  return request.get<AlertInhibit[]>({ url: `${BASE_PATH}/inhibits` });
}
export function createAlertInhibit(data: Omit<AlertInhibit, "id" | "created_at" | "updated_at">) {
  return request.post<AlertInhibit>({ url: `${BASE_PATH}/inhibits`, data });
}
export function updateAlertInhibit(
  id: string,
  data: Omit<AlertInhibit, "id" | "created_at" | "updated_at">,
) {
  return request.put<AlertInhibit>({ url: `${BASE_PATH}/inhibits/${id}`, data });
}
export function deleteAlertInhibit(id: string) {
  return request.del<{ id: string }>({ url: `${BASE_PATH}/inhibits/${id}` });
}

