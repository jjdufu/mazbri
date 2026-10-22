import request from "@/utils/http";
import { API_BASE_PATH } from "@/config/api";

const BASE = `${API_BASE_PATH}/upstream-connectors`;

export interface ConnectorEndpoint {
  id?: string;
  name?: string;
  method?: string;
  path?: string;
  headers?: Record<string, string>;
  query?: Record<string, string>;
  header_entries?: Array<{
    key: string;
    value: string;
    desc?: string;
    enabled?: boolean;
  }>;
  query_entries?: Array<{
    key: string;
    value: string;
    desc?: string;
    enabled?: boolean;
  }>;
  body?: unknown;
  /** L1: none | json. Never a MIME. */
  body_content_type?: string;
  /** L2: json | form | none. */
  body_encoding?: string;
  /** L2 JSON MIME override. */
  content_type?: string;
  channel_code?: string;
  path_prefix?: string;
  assertions?: AssertionSpec[];
  gate?: boolean;
  adapter?: string;
  operation?: string;
  wrap?: string;
  unwrap?: string;
  base_url?: string;
  timeout_ms?: number;
  artifact_id?: string;
  auth?: Record<string, unknown>;
  graphql?: Record<string, unknown>;
  job_poll?: Record<string, unknown>;
  ws?: Record<string, unknown>;
  soap?: Record<string, unknown>;
  sign_form?: Record<string, unknown>;
  egress?: Record<string, unknown>;

  tag?: string;
}

export interface AssertionSpec {
  type: string;
  values?: number[];
  min?: number;
  max?: number;
  value?: number | string;
  path?: string;
  expected?: unknown;
  pattern?: string;
  key?: string;
  name?: string;
  /** Client Sortable key. Engine ignores it. */
  uid?: string;
}

export type WorkbenchEnvEntry = {
  key?: string;
  value?: string;
  desc?: string;
  enabled?: boolean;
};

/** Array of KV rows (incl. disabled) or legacy string map. */
export type WorkbenchEnvValue = WorkbenchEnvEntry[];

export interface ConnectorDraft {
  id: string;
  draft_code: string;
  name: string;
  description?: string | null;
  upstream_type: string;
  upstream_url: string;
  config: Record<string, unknown>;
  endpoints: ConnectorEndpoint[];
  workbench_env?: WorkbenchEnvValue;
  status: string;
  source: string;
  linked_supplier_id?: string | null;
  created_by?: string | null;
  created_at: string;
  updated_at: string;
  gate?: DraftGateItem[];
}

export type DraftGateReason =
  | "not_run"
  | "spec_changed"
  | "unresolved_vars"
  | "skipped"
  | "";

export interface DraftGateItem {
  endpoint_key: string;
  fingerprint?: string | null;
  met: boolean;
  reason?: DraftGateReason;
}

export interface ConnectorRun {
  id: string;
  draft_id?: string | null;
  supplier_id?: string | null;
  mode: string;
  ok: boolean;
  latency_ms?: number | null;
  stages: StageResult[];
  assertions: AssertionResult[];
  error_code?: string | null;
  error_message?: string | null;
  endpoint_key?: string | null;
  actor?: string | null;
  created_at: string;
  spec_fingerprint?: string | null;
  response_preview?: {
    status?: number;
    body_preview?: string;
    url?: string;
    headers?: Record<string, string>;
  };
}

export interface StageResult {
  name: string;
  ok: boolean;
  latency_ms: number;
  request_summary?: Record<string, unknown>;
  response_summary?: Record<string, unknown>;
  error?: string | null;
}

export interface AssertionResult {
  name: string;
  ok: boolean;
  message?: string | null;
}

export interface PublishRequest {
  mode: "supplier_only" | "supplier_channels" | "full_onboarding";
  supplier: {
    supplier_code: string;
    name: string;
    status?: string;
  };
  channels?: Array<{
    endpoint_id?: string;
    channel_code: string;
    name: string;
    path_prefix?: string;
    cost_price?: string;
    qps_limit?: number;
  }>;
  route?: {
    product_ref: { product_id: string };
    tenant_id?: string | null;
    priority?: number;
    weight?: number;
    version_tag?: string;
  };
  authorization?: { tenant_ids?: string[]; enabled?: boolean };
  options?: {
    require_gate?: boolean;
    force_publish?: boolean;
    force_reason?: string | null;
    fail_if_route_exists?: boolean;
    probe_connectivity?: boolean;
  };
  comment?: string;
}

export interface PublishResult {
  supplier_id: string;
  channel_ids: string[];
  route_rule_id?: string | null;
  binding_ids: string[];
  route_version?: number | null;
  version_id: string;
  version_no: number;
  warnings: string[];
  sample_curl?: string | null;
}

export type DraftListPagination = {
  page: number;
  page_size: number;
  total: number;
  total_pages: number;
};

export function listDrafts(params?: {
  q?: string;
  status?: string;
  page?: number;
  page_size?: number;
}) {
  return request.get<{
    drafts: ConnectorDraft[];
    pagination: DraftListPagination;
  }>({
    url: `${BASE}/drafts`,
    params,
  });
}

export function createDraft(
  data: {
    draft_code: string;
    name: string;
    description?: string;
    upstream_type?: string;
    upstream_url?: string;
    config?: Record<string, unknown>;
    endpoints?: ConnectorEndpoint[];
    workbench_env?: WorkbenchEnvValue;
    source?: string;
  },
  opts?: ConnectorRequestOpts,
) {
  return request.post<ConnectorDraft>({
    url: `${BASE}/drafts`,
    data,
    ...withConnectorOpts(opts),
  });
}

export function getDraft(id: string) {
  return request.get<ConnectorDraft>({
    url: `${BASE}/drafts/${id}`,
  });
}

export type ConnectorRequestOpts = {
  signal?: AbortSignal;
  showErrorMessage?: boolean;
  timeout?: number;
};

function withConnectorOpts(opts?: ConnectorRequestOpts): ConnectorRequestOpts {
  if (!opts) return {};
  const out: ConnectorRequestOpts = {};
  if (opts.signal) out.signal = opts.signal;
  if (opts.showErrorMessage !== undefined) {
    out.showErrorMessage = opts.showErrorMessage;
  }
  if (opts.timeout != null) out.timeout = opts.timeout;
  return out;
}

export function updateDraft(
  id: string,
  data: Partial<{
    name: string;
    description: string;
    upstream_type: string;
    upstream_url: string;
    config: Record<string, unknown>;
    endpoints: ConnectorEndpoint[];
    workbench_env: WorkbenchEnvValue;
    status: string;
  }>,
  opts?: ConnectorRequestOpts,
) {
  return request.patch<ConnectorDraft>({
    url: `${BASE}/drafts/${id}`,
    data,
    ...withConnectorOpts(opts),
  });
}

export function deleteDraft(id: string) {
  return request.del({
    url: `${BASE}/drafts/${id}`,
  });
}

export function runDraft(
  id: string,
  data: {
    mode?: string;
    endpoint_id?: string;
    assertions?: AssertionSpec[];
  },
  opts?: ConnectorRequestOpts,
) {
  return request.post<ConnectorRun>({
    url: `${BASE}/drafts/${id}/runs`,
    data,
    ...withConnectorOpts(opts),
  });
}

export function listRuns(id: string) {
  return request.get<ConnectorRun[]>({
    url: `${BASE}/drafts/${id}/runs`,
  });
}

export function getRun(runId: string) {
  return request.get<ConnectorRun>({
    url: `${BASE}/runs/${runId}`,
  });
}

export function transformPreview(
  id: string,
  data: {
    endpoint_id?: string;
  },
  opts?: ConnectorRequestOpts,
) {
  return request.post<{ ok: boolean; stages: StageResult[] }>({
    url: `${BASE}/drafts/${id}/transform-preview`,
    data,
    ...withConnectorOpts(opts),
  });
}

export function publishDraft(id: string, data: PublishRequest) {
  return request.post<PublishResult>({
    url: `${BASE}/drafts/${id}/publish`,
    data,
  });
}

export function importCurl(data: { curl: string; draft_code?: string; name?: string }) {
  return request.post<ConnectorDraft>({
    url: `${BASE}/import/curl`,
    data,
  });
}

export function importPostman(data: {
  collection: unknown;
  draft_code?: string;
  name?: string;
}) {
  return request.post<ConnectorDraft>({
    url: `${BASE}/import/postman`,
    data,
  });
}

export function importOpenApi(data: {
  document: unknown;
  draft_code?: string;
  name?: string;
}) {
  return request.post<ConnectorDraft>({
    url: `${BASE}/import/openapi`,
    data,
  });
}

export function draftFromSupplier(supplierId: string) {
  return request.post<ConnectorDraft>({
    url: `${BASE}/drafts/from-supplier/${supplierId}`,
  });
}

export function exportPostman(id: string) {
  return request.get<unknown>({
    url: `${BASE}/drafts/${id}/export/postman`,
  });
}

export function listVersions(supplierId: string) {
  return request.get<
    Array<{
      id: string;
      version_no: number;
      comment?: string;
      published_by?: string;
      created_at: string;
    }>
  >({
    url: `${BASE}/suppliers/${supplierId}/versions`,
  });
}

export function versionDiff(supplierId: string, versionId: string) {
  return request.get<Record<string, unknown>>({
    url: `${BASE}/suppliers/${supplierId}/versions/${versionId}/diff`,
  });
}

export function rollbackVersion(
  supplierId: string,
  data: { version_id: string; reason?: string },
) {
  return request.post<{ ok: boolean; version_id: string; version_no: number }>({
    url: `${BASE}/suppliers/${supplierId}/rollback`,
    data,
  });
}

export function liveRun(
  supplierId: string,
  data: {
    channel_code: string;
    payload?: unknown;
  },
  opts?: ConnectorRequestOpts,
) {
  return request.post<ConnectorRun>({
    url: `${BASE}/suppliers/${supplierId}/runs`,
    data,
    ...withConnectorOpts(opts),
  });
}

export interface UpstreamArtifact {
  id: string;
  artifact_type: string;
  name: string;
  content_sha256: string;
  size_bytes: number;
  created_at: string;
  updated_at: string;
}

export function uploadArtifact(data: {
  artifact_type: string;
  name: string;
  content_base64: string;
}) {
  return request.post<UpstreamArtifact>({
    url: `${BASE}/artifacts`,
    data,
  });
}

export function getArtifact(id: string) {
  return request.get<UpstreamArtifact>({
    url: `${BASE}/artifacts/${id}`,
  });
}
