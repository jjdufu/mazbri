import type { ISODateString } from "./common";
export interface ConfigEntry {
  id: string;
  service_name: string;
  content: string;
  content_hash: string | null;
  is_sensitive?: boolean;
  updated_by: string;
  created_at: ISODateString;
  updated_at: ISODateString;
}
export interface ConfigHistory {
  id: string;
  config_id: string | null;
  service_name: string;
  content: string;
  content_hash: string | null;
  version_number: number;
  updated_by: string;
  created_at: ISODateString;
}
export interface ConfigListQuery {
  service_name?: string;
  q?: string;

  filters?: string;
  page?: number;
  page_size?: number;
}
export interface ConfigListResponse {
  configs: ConfigEntry[];
  total: number;
  page: number;
  page_size: number;
}
export interface CreateConfigRequest {
  service_name: string;
  content: string;
  is_sensitive?: boolean;
  updated_by: string;
}
export interface UpdateConfigRequest {
  content: string;
  updated_by?: string;
}
export interface ServiceListResponse {
  services: string[];
}
export interface HistoryListQuery {
  service_name: string;
  page?: number;
  page_size?: number;
}
export interface HistoryListResponse {
  history: ConfigHistory[];
  total: number;
  page: number;
  page_size: number;
}
export interface RollbackRequest {
  service_name: string;
  version: number;
}
export interface ExportQuery {
  service_names?: string[];
}
export interface ImportConfigsResult {
  imported: number;
  skipped: number;
  errors: string[];
  total_extracted_bytes?: number;
}
export interface NodeInfo {
  id: string;
  service_name: string;
  ip: string;
  port: number;
  started_at: ISODateString;
  config_version: number;
  config_md5: string | null;
  health_check_url: string | null;
  last_heartbeat_at: ISODateString;
  created_at: ISODateString;
}
export interface NodeListResponse {
  nodes: NodeInfo[];
  total: number;
}
export type NodeListQuery = Record<string, never>;
