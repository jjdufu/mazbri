import type { Supplier } from "@/api/supplier";
import { stripUrlScheme } from "@/utils/sensitive";
import {
  createEmptyInvalidation,
  invalidationFromApi,
  invalidationToApi,
  type InvalidationForm,
} from "@/views/mazbri/upstream-connectors/authInvalidation";

export interface AuthJwtForm {
  algorithm: string;
  ttl_secs: number;
  claims_tpl: string;
  kid: string;
}

export interface AuthDigestForm {
  algorithm: string;
  realm: string;
}

export interface AuthAwsForm {
  region: string;
  service: string;
}

export interface AuthHawkForm {
  algorithm: string;
  ext: string;
}

export interface AuthFlowForm {
  enabled: boolean;
  profile: string;
  token_url: string;
  token_path: string;
  method: string;
  headers: { key: string; value: string }[];
  request_body_tpl: string;
  credentials: { key: string; value: string }[];
  token_jsonpath: string;
  expiry_jsonpath: string;
  inject_to: string;
  inject_key: string;
  inject_value_format: string;
  jwt: AuthJwtForm;
  digest: AuthDigestForm;
  aws: AuthAwsForm;
  hawk: AuthHawkForm;
  invalidation: InvalidationForm;
}

export interface SupplierConfigForm {
  upstream_type: string;
  contract_id?: string;
  auth_flow: AuthFlowForm;
}

export type UpstreamType = "microservice" | "transparent";

export interface SupplierFormModel {
  supplier_code: string;
  name: string;
  upstream_url: string;
  status: string;
  upstream_type: UpstreamType;
  config: SupplierConfigForm;
}

export function createEmptyAuthFlow(): AuthFlowForm {
  return {
    enabled: false,
    profile: "",
    token_url: "",
    token_path: "",
    method: "POST",
    headers: [],
    request_body_tpl: "",
    credentials: [],
    token_jsonpath: "$.token",
    expiry_jsonpath: "$.expires_in",
    inject_to: "header",
    inject_key: "Authorization",
    inject_value_format: "Bearer {}",
    jwt: { algorithm: "HS256", ttl_secs: 300, claims_tpl: '{"sub":"mazbri"}', kid: "" },
    digest: { algorithm: "MD5", realm: "" },
    aws: { region: "", service: "" },
    hawk: { algorithm: "sha256", ext: "" },
    invalidation: createEmptyInvalidation(),
  };
}

function mapToKvList(obj?: Record<string, string> | null): { key: string; value: string }[] {
  if (!obj || typeof obj !== "object") return [];
  return Object.entries(obj).map(([key, value]) => ({
    key,
    value: String(value ?? ""),
  }));
}

function kvListToMap(list: { key: string; value: string }[]): Record<string, string> {
  const out: Record<string, string> = {};
  list.forEach((item) => {
    if (item.key.trim()) out[item.key.trim()] = item.value;
  });
  return out;
}

export function createEmptySupplierForm(): SupplierFormModel {
  return {
    supplier_code: "",
    name: "",
    upstream_url: "",
    status: "active",
    upstream_type: "microservice",
    config: {
      upstream_type: "microservice",
      contract_id: "",
      auth_flow: createEmptyAuthFlow(),
    },
  };
}

export function resolveUpstreamType(supplier: Supplier): UpstreamType {
  return supplier.config?.upstream_type === "transparent"
    ? "transparent"
    : "microservice";
}

export function supplierToForm(supplier: Supplier): SupplierFormModel {
  const type = resolveUpstreamType(supplier);
  const credsList: { key: string; value: string }[] = [];
  if (supplier.config?.auth_flow?.credentials) {
    Object.entries(supplier.config.auth_flow.credentials).forEach(([k, v]) => {
      credsList.push({ key: k, value: String(v) });
    });
  }

  const rawProfile = String(supplier.config?.auth_flow?.profile || "");
  const rawTokenUrl = String(supplier.config?.auth_flow?.token_url || "");
  const activeHttpAuth =
    Boolean(supplier.config?.auth_flow?.enabled) &&
    Boolean((rawProfile && rawProfile !== "none" && rawProfile !== "skip") || rawTokenUrl.trim());

  return {
    supplier_code: supplier.supplier_code,
    name: supplier.name,
    upstream_url:
      type === "transparent"
        ? supplier.upstream_url
        : stripUrlScheme(supplier.upstream_url),
    status: supplier.status,
    upstream_type: type,
    config: {
      upstream_type: type,
      contract_id:
        (supplier.config as { contract_id?: string } | undefined)?.contract_id ||
        "",
      auth_flow: {
        ...createEmptyAuthFlow(),
        enabled: activeHttpAuth,
        profile: rawProfile === "none" ? "" : rawProfile,
        token_url: rawTokenUrl,
        token_path: String(
          (supplier.config?.auth_flow as { token_path?: string } | undefined)?.token_path || "",
        ),
        method: supplier.config?.auth_flow?.method || "POST",
        headers: mapToKvList(
          supplier.config?.auth_flow?.headers as Record<string, string> | undefined,
        ),
        request_body_tpl: supplier.config?.auth_flow?.request_body_tpl || "",
        credentials: credsList,
        token_jsonpath:
          supplier.config?.auth_flow?.token_jsonpath || "$.token",
        expiry_jsonpath:
          supplier.config?.auth_flow?.expiry_jsonpath || "$.expires_in",
        inject_to: supplier.config?.auth_flow?.inject_to || "header",
        inject_key: supplier.config?.auth_flow?.inject_key || "Authorization",
        inject_value_format:
          supplier.config?.auth_flow?.inject_value_format || "Bearer {}",
        jwt: {
          ...createEmptyAuthFlow().jwt,
          ...(((supplier.config?.auth_flow as { jwt?: Record<string, unknown> } | undefined)?.jwt) || {}),
        } as AuthFlowForm["jwt"],
        digest: {
          ...createEmptyAuthFlow().digest,
          ...(((supplier.config?.auth_flow as { digest?: Record<string, unknown> } | undefined)?.digest) || {}),
        } as AuthFlowForm["digest"],
        aws: {
          ...createEmptyAuthFlow().aws,
          ...(((supplier.config?.auth_flow as { aws?: Record<string, unknown> } | undefined)?.aws) || {}),
        } as AuthFlowForm["aws"],
        hawk: {
          ...createEmptyAuthFlow().hawk,
          ...(((supplier.config?.auth_flow as { hawk?: Record<string, unknown> } | undefined)?.hawk) || {}),
        } as AuthFlowForm["hawk"],
        invalidation: invalidationFromApi(
          (supplier.config?.auth_flow as { invalidation?: unknown } | undefined)?.invalidation,
        ),
      },
    },
  };
}

export function serializeAuthFlow(
  flow: AuthFlowForm,
  credsMap: Record<string, string>,
): Record<string, unknown> {
  const profile = String(flow.profile || "").trim();
  const out: Record<string, unknown> = {
    enabled: true,
    token_url: flow.token_url.trim(),
    method: flow.method,
    headers: kvListToMap(flow.headers || []),
    request_body_tpl: flow.request_body_tpl,
    credentials: credsMap,
    token_jsonpath: flow.token_jsonpath.trim(),
    expiry_jsonpath: flow.expiry_jsonpath.trim(),
    inject_to: flow.inject_to,
    inject_key: flow.inject_key.trim(),
    inject_value_format: flow.inject_value_format,
  };
  if (profile && profile !== "none") out.profile = profile;
  if (flow.token_path?.trim()) out.token_path = flow.token_path.trim();
  const invalidation = invalidationToApi(flow.invalidation);
  if (invalidation) out.invalidation = invalidation;
  if (profile === "jwt" && flow.jwt) {
    out.jwt = {
      algorithm: flow.jwt.algorithm || "HS256",
      ttl_secs: flow.jwt.ttl_secs || 300,
      claims_tpl: flow.jwt.claims_tpl || '{"sub":"mazbri"}',
      kid: flow.jwt.kid || undefined,
    };
  }
  if (profile === "digest" && flow.digest) {
    out.digest = {
      algorithm: flow.digest.algorithm || "MD5",
      realm: flow.digest.realm || undefined,
    };
  }
  if (profile === "aws_sigv4" && flow.aws) {
    out.aws = { region: flow.aws.region, service: flow.aws.service };
  }
  if (profile === "hawk" && flow.hawk) {
    out.hawk = {
      algorithm: flow.hawk.algorithm || "sha256",
      ext: flow.hawk.ext || undefined,
    };
  }
  return out;
}

export function buildSupplierUpdatePayload(form: SupplierFormModel) {
  const credsMap: Record<string, string> = {};
  form.config.auth_flow.credentials.forEach((item) => {
    if (item.key.trim()) {
      credsMap[item.key.trim()] = item.value;
    }
  });

  const hasCreds = Object.keys(credsMap).length > 0;
  const configPayload: Record<string, unknown> = {
    upstream_type: form.upstream_type,
    auth_flow: form.config.auth_flow.enabled
      ? serializeAuthFlow(form.config.auth_flow, credsMap)
      : hasCreds
        ? { enabled: false, profile: "none", credentials: credsMap }
        : null,
  };
  if (form.upstream_type === "microservice") {
    configPayload.contract_id = (form.config.contract_id || "").trim();
  }

  return {
    supplier_code: form.supplier_code.trim(),
    name: form.name.trim(),
    upstream_url: form.upstream_url.trim(),
    status: form.status,
    config: configPayload,
  };
}
