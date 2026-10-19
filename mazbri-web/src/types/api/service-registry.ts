import type { ISODateString } from "./common";
export type ServiceSource = "system" | "user";
export type ServiceCategory =
  | "gateway"
  | "auth"
  | "cache"
  | "ratelimit"
  | "billing"
  | "analytics"
  | "platform"
  | "other";
export type ServiceStatus = "active" | "deprecated" | "retired";
export interface Service {
  service_name: string;
  display_name: string;
  description: string | null;
  category: ServiceCategory;
  status: ServiceStatus;
  source: ServiceSource;
  dns_names: string[];
  metadata: Record<string, string> | null;
  created_at: ISODateString;
  updated_at: ISODateString;
}
export interface CreateServiceRequest {
  service_name: string;
  display_name: string;
  description?: string;
  category: ServiceCategory;
  dns_names: string[];
  metadata?: Record<string, string>;
}
export interface UpdateServiceRequest {
  display_name?: string;
  description?: string;
  category?: ServiceCategory;
  dns_names?: string[];
  metadata?: Record<string, string>;
}
export interface ServiceValidationResponse {
  valid: boolean;
  service?: Service;
  reason?: string;
}
export interface ServiceNameQuery {
  status?: "active" | "deprecated" | "retired";
  source?: ServiceSource;
}
