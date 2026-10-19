export type AuthFlowLike = {
  credentials: { key: string; value: string }[];
  method: string;
  token_jsonpath: string;
  expiry_jsonpath: string;
  inject_to: string;
  inject_value_format: string;
  jwt?: { algorithm: string; ttl_secs: number; claims_tpl: string; kid: string };
  digest?: { algorithm: string; realm: string };
  aws?: { region: string; service: string };
  hawk?: { algorithm: string; ext: string };
};

export const AUTH_PROFILES = [
  "none",
  "api_key",
  "oauth_cc_form",
  "gettoken",
  "basic",
  "sign_form",
  "grpc_metadata",
  "bearer",
  "jwt",
  "digest",
  "aws_sigv4",
  "hawk",
] as const;

export const AUTH_PICKER_IDS = ["inherit", ...AUTH_PROFILES] as const;

export type AuthPickerId = (typeof AUTH_PICKER_IDS)[number];
export type AuthProfileId = (typeof AUTH_PROFILES)[number];

export type AuthFieldGroup =
  | "empty"
  | "basic"
  | "api_key"
  | "ticket"
  | "sign_form"
  | "grpc_metadata"
  | "bearer"
  | "jwt"
  | "digest"
  | "aws_sigv4"
  | "hawk";

export function authFieldGroup(picker: AuthPickerId): AuthFieldGroup {
  switch (picker) {
    case "none":
    case "inherit":
      return "empty";
    case "basic":
      return "basic";
    case "api_key":
      return "api_key";
    case "oauth_cc_form":
    case "gettoken":
      return "ticket";
    case "sign_form":
      return "sign_form";
    case "grpc_metadata":
      return "grpc_metadata";
    case "bearer":
      return "bearer";
    case "jwt":
      return "jwt";
    case "digest":
      return "digest";
    case "aws_sigv4":
      return "aws_sigv4";
    case "hawk":
      return "hawk";
    default:
      return "empty";
  }
}

export function visibleAuthPickers(flowEnabled: boolean): AuthPickerId[] {
  return AUTH_PICKER_IDS.filter((id) => {
    if (id === "inherit") return flowEnabled;
    if (id === "none") return !flowEnabled;
    return true;
  });
}

export function authPickerFromState(
  profile: string | undefined | null,
  flowEnabled: boolean,
): AuthPickerId {
  const p = String(profile || "none").trim() || "none";
  if (p === "none") return flowEnabled ? "inherit" : "none";
  if ((AUTH_PROFILES as readonly string[]).includes(p) && p !== "none") {
    return p as AuthPickerId;
  }
  return flowEnabled ? "inherit" : "none";
}

export function authProfileFromPicker(picker: AuthPickerId): string {
  if (picker === "inherit" || picker === "none") return "none";
  return picker;
}

export function sessionProfileLabel(profile: string | undefined | null): string {
  const p = String(profile || "").trim();
  if (!p || p === "none") return "";
  return p;
}

export function ensureCredentialKeys(
  flow: AuthFlowLike,
  keys: string[],
): void {
  if (!Array.isArray(flow.credentials)) flow.credentials = [];
  for (const key of keys) {
    if (!flow.credentials.some((c) => c.key === key)) {
      flow.credentials.push({ key, value: "" });
    }
  }
}

export function credValue(flow: AuthFlowLike, key: string): string {
  const row = (flow.credentials || []).find((c) => c.key === key);
  return row ? String(row.value ?? "") : "";
}

export function setCredValue(flow: AuthFlowLike, key: string, value: string): void {
  ensureCredentialKeys(flow, [key]);
  const row = flow.credentials.find((c) => c.key === key);
  if (row) row.value = value;
}

export function namedKeysForPicker(picker: AuthPickerId): string[] {
  switch (picker) {
    case "basic":
    case "digest":
      return ["username", "password"];
    case "api_key":
      return ["api_key"];
    case "oauth_cc_form":
      return ["client_id", "client_secret"];
    case "gettoken":
      return ["app_id", "secret"];
    case "sign_form":
      return ["app_id", "client_secret"];
    case "grpc_metadata":
      return ["api_key", "secret"];
    case "bearer":
      return ["token"];
    case "jwt":
      return ["jwt_secret", "jwt_private_key"];
    case "aws_sigv4":
      return ["access_key_id", "secret_access_key", "session_token"];
    case "hawk":
      return ["hawk_id", "hawk_key"];
    default:
      return [];
  }
}

export function applyPickerDefaults(picker: AuthPickerId, flow: AuthFlowLike): void {
  ensureCredentialKeys(flow, namedKeysForPicker(picker));
  if (picker === "oauth_cc_form") {
    if (!String(flow.method || "").trim()) flow.method = "POST";
    if (!String(flow.token_jsonpath || "").trim()) flow.token_jsonpath = "$.access_token";
    if (!String(flow.expiry_jsonpath || "").trim()) flow.expiry_jsonpath = "$.expires_in";
    if (!String(flow.inject_to || "").trim()) flow.inject_to = "header";
    if (!String(flow.inject_value_format || "").trim()) {
      flow.inject_value_format = "Bearer {}";
    }
  }
  if (picker === "gettoken") {
    if (!String(flow.method || "").trim()) flow.method = "GET";
    if (!String(flow.token_jsonpath || "").trim()) flow.token_jsonpath = "$.token";
    if (!String(flow.inject_to || "").trim()) flow.inject_to = "header";
    if (!String(flow.inject_value_format || "").trim()) {
      flow.inject_value_format = "Bearer {}";
    }
  }
  if (picker === "api_key" || picker === "bearer" || picker === "jwt") {
    if (!String(flow.inject_to || "").trim()) flow.inject_to = "header";
    if (!String(flow.inject_value_format || "").trim()) {
      flow.inject_value_format = "Bearer {}";
    }
  }
  if (picker === "grpc_metadata") {
    flow.inject_to = "metadata";
    if (!String(flow.inject_value_format || "").trim()) flow.inject_value_format = "{}";
  }
  if (picker === "jwt") {
    if (!flow.jwt) flow.jwt = { algorithm: "HS256", ttl_secs: 300, claims_tpl: '{"sub":"mazbri"}', kid: "" };
    if (!flow.jwt.algorithm) flow.jwt.algorithm = "HS256";
  }
  if (picker === "digest") {
    if (!flow.digest) flow.digest = { algorithm: "MD5", realm: "" };
  }
  if (picker === "aws_sigv4") {
    if (!flow.aws) flow.aws = { region: "", service: "" };
  }
  if (picker === "hawk") {
    if (!flow.hawk) flow.hawk = { algorithm: "sha256", ext: "" };
  }
}

export type AuthSpecDraft = Record<string, unknown>;

function emptyStr(v: unknown): boolean {
  return v == null || String(v).trim() === "";
}

export function sanitizeAuthSpec(spec: AuthSpecDraft | null | undefined): AuthSpecDraft | null {
  if (!spec || typeof spec !== "object") return null;
  const profile = String(spec.profile || "none").trim() || "none";
  if (profile === "none" || profile === "") return null;
  const out: AuthSpecDraft = { profile };
  const copyIf = (key: string) => {
    if (spec[key] != null && !emptyStr(spec[key])) out[key] = spec[key];
  };
  switch (profile) {
    case "basic":
      if (spec.headers && typeof spec.headers === "object") out.headers = spec.headers;
      break;
    case "api_key":
    case "bearer":
      if (spec.inject) out.inject = spec.inject;
      if (spec.credential_keys) out.credential_keys = spec.credential_keys;
      break;
    case "oauth_cc_form":
    case "gettoken":
      copyIf("token_path");
      copyIf("token_url");
      copyIf("token_method");
      copyIf("token_jsonpath");
      copyIf("expiry_jsonpath");
      copyIf("request_body_tpl");
      if (spec.headers) out.headers = spec.headers;
      if (spec.inject) out.inject = spec.inject;
      if (spec.credential_keys) out.credential_keys = spec.credential_keys;
      break;
    case "sign_form":
      break;
    case "grpc_metadata": {
      const inject = (spec.inject && typeof spec.inject === "object"
        ? { ...(spec.inject as Record<string, unknown>) }
        : {}) as Record<string, unknown>;
      inject.to = "metadata";
      out.inject = inject;
      break;
    }
    case "jwt":
      if (spec.jwt) out.jwt = spec.jwt;
      if (spec.inject) out.inject = spec.inject;
      break;
    case "digest":
      if (spec.digest) out.digest = spec.digest;
      break;
    case "aws_sigv4":
      if (spec.aws) out.aws = spec.aws;
      break;
    case "hawk":
      if (spec.hawk) out.hawk = spec.hawk;
      break;
    default:
      return { ...spec, profile };
  }
  return out;
}

export function usesTicket(profile: string): boolean {
  return profile === "oauth_cc_form" || profile === "gettoken";
}

export const INJECT_TO_OPTIONS = ["header", "query", "metadata", "first_frame"] as const;
