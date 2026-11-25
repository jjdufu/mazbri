import type { ISODateString } from "./common";
import type { UpdateApiKeyRequest as GeneratedUpdateApiKeyRequest } from "./generated/auth";

export type ApiScope = "api:read" | "api:write" | "api:admin";

export interface ApiKey {
  key_prefix: string;
  key_suffix: string;
  display_key: string;
  full_key?: string;
  scopes: string[];
  is_active: boolean;
  created_at: ISODateString;
  expires_at?: ISODateString;
}

export interface ApiKeyListResponse {
  keys: ApiKey[];
  total: number;
  actions?: {
    create: boolean;
    rotate?: boolean;
    delete: boolean;
  };
}

export interface CreateApiKeyRequest {
  scopes: ApiScope[];
  expires_at?: ISODateString;
  expires_days?: number;
}

export interface CreateApiKeyResponse {
  api_key: string;
  key_info: ApiKey;
}

export interface RotateApiKeyResponse {
  api_key: string;
  previous_key_prefix: string;
  key_info: ApiKey;
}

export type UpdateApiKeyRequest = {
  [K in keyof GeneratedUpdateApiKeyRequest]?: GeneratedUpdateApiKeyRequest[K] extends
    | bigint
    | null
    ? number | null
    : GeneratedUpdateApiKeyRequest[K];
};

export interface UpdateApiKeyResponse {
  success: boolean;
  key_info: ApiKey;
}
