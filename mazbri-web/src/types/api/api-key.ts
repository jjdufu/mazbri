import type { ISODateString } from "./common";
export interface ApiKey {
  key_prefix: string;
  key_suffix: string;
  display_key: string;
  full_key?: string;
  scopes: string[];
  is_active: boolean;
  created_at: ISODateString;
  expires_at?: ISODateString;
  warmup_enabled: boolean;
}
export interface ApiKeyListResponse {
  keys: ApiKey[];
  total: number;
  actions?: {
    create: boolean;
    delete: boolean;
  };
}
export interface CreateApiKeyRequest {
  scopes: string[];
  expires_at?: ISODateString;
  expires_days?: number;
  warmup_enabled?: boolean;
}
export interface CreateApiKeyResponse {
  api_key: string;
  key_info: ApiKey;
}
export interface UpdateApiKeyRequest {
  expires_days?: number;
}
export interface UpdateApiKeyResponse {
  success: boolean;
  key_info: ApiKey;
}
