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

export interface OncallSchedule {
  id: string;
  name: string;
  timezone: string;
  layers: unknown[];
  escalation: unknown[];
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface NotifyChannel {
  id: string;
  name: string;
  type: "email" | "webhook" | "dingtalk" | "feishu" | "wecom";
  config: Record<string, unknown>;
  has_secret: boolean;
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

export function fetchOncallSchedules() {
  return request.get<OncallSchedule[]>({ url: `${BASE_PATH}/oncall` });
}
export function createOncallSchedule(data: {
  name: string;
  timezone?: string;
  layers?: unknown[];
  escalation?: unknown[];
  enabled?: boolean;
}) {
  return request.post<OncallSchedule>({ url: `${BASE_PATH}/oncall`, data });
}
export function updateOncallSchedule(
  id: string,
  data: {
    name: string;
    timezone?: string;
    layers?: unknown[];
    escalation?: unknown[];
    enabled?: boolean;
  },
) {
  return request.put<OncallSchedule>({ url: `${BASE_PATH}/oncall/${id}`, data });
}
export function deleteOncallSchedule(id: string) {
  return request.del<{ id: string }>({ url: `${BASE_PATH}/oncall/${id}` });
}

export function fetchNotifyChannels() {
  return request.get<NotifyChannel[]>({ url: `${BASE_PATH}/channels` });
}
export function createNotifyChannel(data: {
  name: string;
  type: NotifyChannel["type"];
  config?: Record<string, unknown>;
  enabled?: boolean;
}) {
  return request.post<NotifyChannel>({ url: `${BASE_PATH}/channels`, data });
}
export function updateNotifyChannel(
  id: string,
  data: {
    name: string;
    type: NotifyChannel["type"];
    config?: Record<string, unknown>;
    enabled?: boolean;
  },
) {
  return request.put<NotifyChannel>({ url: `${BASE_PATH}/channels/${id}`, data });
}
export function deleteNotifyChannel(id: string) {
  return request.del<{ id: string }>({ url: `${BASE_PATH}/channels/${id}` });
}
