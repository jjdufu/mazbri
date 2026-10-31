import request from "@/utils/http";
import { API_BASE_PATH } from "@/config/api";
import type {
  AlertEvent,
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

export function ackAlertEvent(id: string) {
  return request.post<AlertEvent>({ url: `${BASE_PATH}/events/${id}/ack` });
}

export function assignAlertEvent(id: string, assignee: string | null) {
  return request.post<AlertEvent>({
    url: `${BASE_PATH}/events/${id}/assign`,
    data: { assignee },
  });
}

export function fetchAlertSummary() {
  return request.get<AlertSummary>({
    url: `${BASE_PATH}/summary`,
    showErrorMessage: false,
  });
}
