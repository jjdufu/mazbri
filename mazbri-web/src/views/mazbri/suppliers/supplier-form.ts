import type { Supplier } from "@/api/supplier";
import type {
  AuthFlowForm,
  SupplierConfigForm,
  TransformForm,
} from "./components/SupplierTransparentConfig.vue";
import { stripUrlScheme } from "@/utils/sensitive";

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
  };
}

export function createEmptyTransform(): TransformForm {
  return {
    enabled: false,
    rewrite_path: "",
    request_body_template: "",
    response_body_template: "",
    request_headers_set: [],
    request_headers_remove: [],
    response_headers_set: [],
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
      transform: createEmptyTransform(),
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
        enabled: supplier.config?.auth_flow?.enabled || false,
        profile: String(supplier.config?.auth_flow?.profile || ""),
        token_url: supplier.config?.auth_flow?.token_url || "",
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
      },
      transform: {
        enabled: supplier.config?.transform?.enabled || false,
        rewrite_path: supplier.config?.transform?.rewrite_path || "",
        request_body_template:
          supplier.config?.transform?.request_body_template || "",
        response_body_template:
          supplier.config?.transform?.response_body_template || "",
        request_headers_set: mapToKvList(
          supplier.config?.transform?.request_headers_set as
            | Record<string, string>
            | undefined,
        ),
        request_headers_remove: Array.isArray(
          supplier.config?.transform?.request_headers_remove,
        )
          ? [...(supplier.config?.transform?.request_headers_remove as string[])]
          : [],
        response_headers_set: mapToKvList(
          supplier.config?.transform?.response_headers_set as
            | Record<string, string>
            | undefined,
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

  const configPayload: Record<string, unknown> = {
    upstream_type: form.upstream_type,
    auth_flow: form.config.auth_flow.enabled
      ? serializeAuthFlow(form.config.auth_flow, credsMap)
      : null,
    transform: form.config.transform.enabled
      ? {
          enabled: true,
          rewrite_path: form.config.transform.rewrite_path
            ? form.config.transform.rewrite_path.trim()
            : null,
          request_body_template: form.config.transform.request_body_template
            ? form.config.transform.request_body_template.trim()
            : null,
          response_body_template: form.config.transform.response_body_template
            ? form.config.transform.response_body_template.trim()
            : null,
          request_headers_set: (() => {
            const m = kvListToMap(form.config.transform.request_headers_set || []);
            return Object.keys(m).length ? m : null;
          })(),
          request_headers_remove: (form.config.transform.request_headers_remove || [])
            .map((s) => s.trim())
            .filter(Boolean),
          response_headers_set: (() => {
            const m = kvListToMap(form.config.transform.response_headers_set || []);
            return Object.keys(m).length ? m : null;
          })(),
        }
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
