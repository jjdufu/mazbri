import { create } from "zustand";
import { saveAs } from "file-saver";
import { $t } from "@/locales";
import {
  type AssertionSpec,
  type ConnectorDraft,
  type ConnectorEndpoint,
  type ConnectorRun,
  type DraftGateItem,
  createDraft,
  draftFromSupplier,
  exportPostman,
  getDraft,
  listDrafts,
  listRuns,
  liveRun,
  previewDraftFromSupplier,
  publishDraft,
  runDraft,
  transformPreview,
  updateDraft,
  uploadArtifact,
} from "@/api/upstream-connectors";
import {
  fetchSupplierById,
  fetchSuppliers,
  type Supplier,
} from "@/api/supplier";
import {
  type SupplierConfigForm,
  createEmptyAuthFlow,
  resolveUpstreamType,
} from "@/domains/upstream-supply/supplier-form";
import {
  canonicalOrigin,
  clearMaskedKvEntries,
  hasOriginChanged,
  isMaskedSecretValue,
} from "@/utils/originPinning";
import { copyWithFeedback } from "@/utils/ui/copy-feedback";
import {
  showErrorToast,
  showInfoToast,
  showSuccessToast,
  showWarningToast,
} from "@/utils/ui/message-guard";
import { isHttpError } from "@/utils/http/error";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { isSessionHttpAuthActive } from "../engine/authMatrix";
import { invalidationFromApi } from "../engine/authInvalidation";
import {
  parseCryptoProfile,
  validateCryptoProfileClient,
} from "../engine/cryptoProfileHelpers";
import {
  CODE_FREE_UPSTREAM_TYPE,
  WRITE_METHODS,
  applyEnv,
  asCodeFreeDraft,
  autoChannelsFromEndpoints,
  blankWorkbenchEndpoint,
  buildCurl,
  composeRequestUrl,
  createNamedEnv,
  defaultBodyContentType,
  emptyTlsForm,
  ensureEndpointAdapterFields,
  entriesToKv,
  exportWorkbenchEnvJson,
  fileToBase64,
  genSessionCode,
  importWorkbenchEnvJson,
  isLegalHttpMethod,
  isMicroserviceUpstream,
  isPersistedDraftId,
  kvToMap,
  mapToKv,
  mergeImportedEndpoints,
  mergeQueryFromMap,
  normalizeCaptureList,
  normalizeWorkbenchEndpoint,
  normalizeWorkbenchEnvDoc,
  parseCurl,
  parseOpenApiDocument,
  parsePostmanCollection,
  recordFromUnknown,
  setBodyEncoding,
  tlsFormToSpec,
  tlsSpecToForm,
  trySplitRequestUrl,
  validateTlsFormClient,
  type KvRow,
  type ParsedImport,
  type SplitDirection,
  type TlsFormState,
  type WorkbenchEnvState,
} from "../engine/workbenchHelpers";
import {
  DEFAULT_TAB_RUNTIME,
  buildDraftSnapshotFromState,
  computeSnapshotSig,
  projectEndpointsFromState,
  selectActiveEp,
  selectActiveEpIdx,
  selectCanWriteWorkbench,
  selectDirty,
  selectRequiredCredKeys,
  selectRuntimeEnvMap,
  selectStaticEnvMap,
  selectUnresolvedAll,
  selectUnresolvedRun,
  staticEndpointGateSigFromState,
} from "./selectors";
import type {
  EndpointTabRuntime,
  PublishFormState,
  SidebarPanelId,
  WorkbenchNavigate,
  WorkbenchState,
  WorkbenchStore,
} from "./types";

const WORKBENCH_PATH = "/operations/upstream-supply/workbench";
const LEGACY_LAYOUT_STORAGE_KEY = "mazbri:wb:ide-layout";
const LAYOUT_STORAGE_KEY = "mazbri:wb:ide-layout:v2";
const RUN_HTTP = { showErrorMessage: false as const, timeout: 120_000 };

// Non-serializable module-level coordination state (outside Zustand)
const tabAbortControllers = new Map<string, AbortController>();
let sessionAbortController: AbortController | null = null;
let sessionSeq = 0;
let initialUpstreamUrl = "";
let initialTokenUrl = "";
let initialEndpointBaseUrls: Record<string, string> = {};
let ephemeralRunIds: string[] = [];
let lastNavigate: WorkbenchNavigate | null = null;

function createTabRuntime(): EndpointTabRuntime {
  return { ...DEFAULT_TAB_RUNTIME };
}

function readPersistedIdeLayout(): {
  sidebarPanel: SidebarPanelId;
  sidebarCollapsed: boolean;
  responseCollapsed: boolean;
  responseSplitDir: SplitDirection;
} {
  const defaults = {
    sidebarPanel: "endpoints" as SidebarPanelId,
    sidebarCollapsed: false,
    responseCollapsed: false,
    responseSplitDir: "horizontal" as SplitDirection,
  };
  if (typeof localStorage === "undefined") return defaults;
  try {
    if (localStorage.getItem(LEGACY_LAYOUT_STORAGE_KEY)) {
      localStorage.removeItem(LEGACY_LAYOUT_STORAGE_KEY);
    }
    const raw = localStorage.getItem(LAYOUT_STORAGE_KEY);
    if (!raw) return defaults;
    const p = JSON.parse(raw) as Record<string, unknown>;
    const panel = p.sidebarPanel;
    const sidebarPanel: SidebarPanelId =
      panel === "endpoints" || panel === "gate" || panel === "history"
        ? panel
        : "endpoints";
    return {
      sidebarPanel,
      sidebarCollapsed: Boolean(p.sidebarCollapsed),
      responseCollapsed: Boolean(p.responseCollapsed),
      responseSplitDir:
        p.responseSplitDir === "vertical" ? "vertical" : "horizontal",
    };
  } catch {
    return defaults;
  }
}

function persistIdeLayout(
  state: Pick<
    WorkbenchState,
    | "activeSidebarPanel"
    | "sidebarCollapsed"
    | "responseCollapsed"
    | "responseSplitDir"
  >,
): void {
  if (typeof localStorage === "undefined") return;
  try {
    const existingRaw = localStorage.getItem(LAYOUT_STORAGE_KEY);
    const existing =
      existingRaw && existingRaw.startsWith("{")
        ? (JSON.parse(existingRaw) as Record<string, unknown>)
        : {};
    localStorage.setItem(
      LAYOUT_STORAGE_KEY,
      JSON.stringify({
        ...existing,
        sidebarPanel:
          state.activeSidebarPanel === "env"
            ? "endpoints"
            : state.activeSidebarPanel,
        sidebarCollapsed: state.sidebarCollapsed,
        responseCollapsed: state.responseCollapsed,
        responseSplitDir: state.responseSplitDir,
      }),
    );
  } catch {
    // Ignore storage quota or restricted context errors
  }
}

function createInitialPublishForm(): PublishFormState {
  return {
    mode: "supplier_channels",
    supplier_code: "",
    supplier_name: "",
    product_id: "",
    require_gate: true,
    force_publish: false,
    force_reason: "",
    comment: "",
  };
}

function createInitialConfigForm(): SupplierConfigForm {
  return {
    upstream_type: CODE_FREE_UPSTREAM_TYPE,
    auth_flow: createEmptyAuthFlow(),
  };
}

function createInitialState(): WorkbenchState {
  const layout = readPersistedIdeLayout();
  return {
    activeSidebarPanel: layout.sidebarPanel,
    sidebarCollapsed: layout.sidebarCollapsed,
    responseCollapsed: layout.responseCollapsed,
    responseSplitDir: layout.responseSplitDir,
    envDrawerOpen: false,

    draft: null,
    recentDrafts: [],
    transparentSuppliers: [],
    switcherLoading: false,
    bootstrapping: false,
    saving: false,
    saveError: false,
    lastSavedAt: null,
    savedSnapshotSig: "",
    isEditMode: false,
    endpoints: [],
    openTabIds: [],
    activeTabId: "",
    tabRuntimeMap: {},
    runs: [],
    envState: {
      activeEnvId: null,
      globalRows: [],
      environments: [],
    },
    capturedVars: {},

    headerRows: [],
    queryRows: [],
    gqlVarRows: [],
    gqlEnumRows: [],
    urlDraft: "",
    urlFocused: false,
    urlMissing: false,
    uploadingArtifact: false,
    configForm: createInitialConfigForm(),
    supplierTimeoutMs: 15000,
    supplierTlsForm: emptyTlsForm(),
    endpointTlsForm: emptyTlsForm(),
    originClearedCredKeys: [],
    passedStaticSigMap: {},

    importing: false,
    publishing: false,
    showImport: false,
    showPublish: false,
    showCommands: false,
    showShortcuts: false,
    importTab: "curl",
    importMode: "merge",
    importCurlText: "",
    importJsonText: "",
    publishForm: createInitialPublishForm(),
    publishChannels: [],
    publishResult: null,
    confirmState: null,
  };
}

function parseApiConfig(cfg: Record<string, unknown>): {
  configForm: SupplierConfigForm;
  supplierTimeoutMs: number;
  supplierTlsForm: TlsFormState;
} {
  const auth = (cfg.auth_flow || {}) as Record<string, unknown>;
  const creds = (auth.credentials || {}) as Record<string, string>;
  const credsList = Object.entries(creds).map(([key, value]) => ({
    key,
    value: String(value ?? ""),
  }));
  const rawProfile = String(auth.profile || "");
  const httpActive = isSessionHttpAuthActive(Boolean(auth.enabled), rawProfile);
  const timeouts = (cfg.timeouts || {}) as Record<string, unknown>;
  const supplierTimeoutMs =
    typeof timeouts.total_ms === "number" && timeouts.total_ms > 0
      ? timeouts.total_ms
      : 15000;
  const supplierTlsForm = tlsSpecToForm(
    cfg.tls as Record<string, unknown> | null | undefined,
  );
  const emptyFlow = createEmptyAuthFlow();
  const configForm: SupplierConfigForm = {
    upstream_type: CODE_FREE_UPSTREAM_TYPE,
    auth_flow: {
      ...emptyFlow,
      enabled: httpActive,
      profile: rawProfile,
      token_url: String(auth.token_url || ""),
      token_path: String(auth.token_path || ""),
      method: String(auth.method || "POST"),
      headers: mapToKv(auth.headers as Record<string, string> | undefined),
      request_body_tpl: String(auth.request_body_tpl || ""),
      credentials: credsList.length ? credsList : emptyFlow.credentials,
      token_jsonpath: String(auth.token_jsonpath || "$.token"),
      expiry_jsonpath: String(auth.expiry_jsonpath || "$.expires_in"),
      inject_to: String(auth.inject_to || "header"),
      inject_key: String(auth.inject_key || "Authorization"),
      inject_value_format: String(auth.inject_value_format || "Bearer {}"),
      jwt: {
        ...emptyFlow.jwt,
        ...((auth.jwt as Record<string, unknown>) || {}),
      } as SupplierConfigForm["auth_flow"]["jwt"],
      digest: {
        ...emptyFlow.digest,
        ...((auth.digest as Record<string, unknown>) || {}),
      } as SupplierConfigForm["auth_flow"]["digest"],
      aws: {
        ...emptyFlow.aws,
        ...((auth.aws as Record<string, unknown>) || {}),
      } as SupplierConfigForm["auth_flow"]["aws"],
      hawk: {
        ...emptyFlow.hawk,
        ...((auth.hawk as Record<string, unknown>) || {}),
      } as SupplierConfigForm["auth_flow"]["hawk"],
      invalidation: invalidationFromApi(auth.invalidation),
    },
  };
  return { configForm, supplierTimeoutMs, supplierTlsForm };
}

function kvBuffersForEndpoint(ep: ConnectorEndpoint | null): {
  headerRows: KvRow[];
  queryRows: KvRow[];
  gqlVarRows: KvRow[];
  gqlEnumRows: KvRow[];
  endpointTlsForm: TlsFormState;
} {
  return {
    headerRows: entriesToKv(ep?.header_entries, ep?.headers || {}),
    queryRows: entriesToKv(ep?.query_entries, ep?.query || {}),
    gqlVarRows: mapToKv(recordFromUnknown(ep?.graphql?.variables_map)),
    gqlEnumRows: mapToKv(recordFromUnknown(ep?.graphql?.enum_map)),
    endpointTlsForm: tlsSpecToForm(ep?.tls ?? null),
  };
}

function flushActiveBuffersIntoEndpoints(
  state: WorkbenchState,
): ConnectorEndpoint[] {
  const activeIdx = selectActiveEpIdx(state);
  if (activeIdx < 0) return state.endpoints;
  const ep = state.endpoints[activeIdx];
  if (!ep) return state.endpoints;
  const nextEp: ConnectorEndpoint = {
    ...ep,
    header_entries: state.headerRows.map((r) => ({
      key: r.key,
      value: r.value,
      desc: r.desc ?? "",
      enabled: r.enabled !== false,
    })),
    query_entries: state.queryRows.map((r) => ({
      key: r.key,
      value: r.value,
      desc: r.desc ?? "",
      enabled: r.enabled !== false,
    })),
    headers: kvToMap(state.headerRows),
    query: kvToMap(state.queryRows),
    tls: tlsFormToSpec(state.endpointTlsForm),
    captures: normalizeCaptureList(ep.captures),
  };
  if (nextEp.adapter === "graphql") {
    ensureEndpointAdapterFields(nextEp);
    nextEp.graphql = {
      ...(nextEp.graphql || {}),
      variables_map: kvToMap(state.gqlVarRows),
      enum_map: kvToMap(state.gqlEnumRows),
    };
  }
  const nextList = [...state.endpoints];
  nextList[activeIdx] = nextEp;
  return nextList;
}

function composeUrlForState(
  draft: ConnectorDraft | null,
  ep: ConnectorEndpoint | null,
  queryRows: KvRow[],
): string {
  const base = (ep?.base_url || "").trim() || draft?.upstream_url || "";
  return composeRequestUrl(base, ep?.path || "", queryRows);
}

function captureOriginBaselineFrom(
  draft: ConnectorDraft | null,
  configForm: SupplierConfigForm,
  endpoints: ConnectorEndpoint[],
): void {
  initialUpstreamUrl = draft?.upstream_url ?? "";
  initialTokenUrl = configForm.auth_flow.token_url ?? "";
  const epBases: Record<string, string> = {};
  endpoints.forEach((ep, idx) => {
    const key = ep.id || `ep-${idx + 1}`;
    epBases[key] = ep.base_url ?? "";
  });
  initialEndpointBaseUrls = epBases;
}

function buildPassedGateSigMap(
  state: WorkbenchState,
  gateItems: DraftGateItem[] | undefined,
): Record<string, string> {
  const projected = projectEndpointsFromState(state);
  const nextMap: Record<string, string> = {};
  projected.forEach((ep, i) => {
    const id = ep.id || `ep-${i + 1}`;
    const hit = (gateItems || []).find(
      (g) =>
        g.endpoint_key &&
        (g.endpoint_key === ep.id ||
          g.endpoint_key === ep.name ||
          g.endpoint_key === ep.path ||
          g.endpoint_key === id),
    );
    if (hit?.met) {
      nextMap[id] = staticEndpointGateSigFromState(state, ep);
    }
  });
  return nextMap;
}

function buildLocalBlankDraft(): ConnectorDraft {
  const now = new Date().toISOString();
  return {
    id: "",
    draft_code: genSessionCode(),
    name: $t("upstreamConnector.untitledName"),
    description: null,
    upstream_type: CODE_FREE_UPSTREAM_TYPE,
    upstream_url: "",
    config: {
      upstream_type: CODE_FREE_UPSTREAM_TYPE,
      timeouts: { total_ms: 15000 },
    },
    endpoints: [blankWorkbenchEndpoint(0)],
    status: "draft",
    source: "manual",
    linked_supplier_id: null,
    created_by: null,
    created_at: now,
    updated_at: now,
  };
}

function isRunCancelled(err: unknown): boolean {
  if (isMfaStepUpCancelled(err)) {
    return true;
  }
  if (isHttpError(err)) {
    return (
      err.cancelled || err.message === $t("httpMsg.requestCancelled")
    );
  }
  return (
    err instanceof DOMException && err.name === "AbortError"
  );
}

function syncRouteForDraft(
  d: ConnectorDraft,
  navigate?: WorkbenchNavigate,
): void {
  const nav = navigate ?? lastNavigate;
  if (!nav) return;
  if (d.linked_supplier_id) {
    nav(
      `${WORKBENCH_PATH}?supplier_id=${encodeURIComponent(d.linked_supplier_id)}`,
      { replace: true },
    );
    return;
  }
  if (!isPersistedDraftId(d.id)) {
    nav(WORKBENCH_PATH, { replace: true });
    return;
  }
  nav(`${WORKBENCH_PATH}?session=${encodeURIComponent(d.id)}`, {
    replace: true,
  });
}

function leaveForMicroservice(
  supplierId?: string | null,
  navigate?: WorkbenchNavigate,
): void {
  showWarningToast($t("upstreamConnector.transparentOnly"));
  const nav = navigate ?? lastNavigate;
  if (!nav) return;
  if (supplierId) {
    nav(`/operations/upstream-supply/suppliers/${encodeURIComponent(supplierId)}`, {
      replace: true,
    });
    return;
  }
  nav("/operations/upstream-supply?tab=suppliers", { replace: true });
}

export const useWorkbenchStore = create<WorkbenchStore>()((set, get) => {
  function runOriginPinningCheck(): void {
    const state = get();
    if (!selectCanWriteWorkbench() || !state.draft || state.bootstrapping) {
      return;
    }
    const env = selectStaticEnvMap(state);
    const baseOrigin = canonicalOrigin(applyEnv(initialUpstreamUrl, env), {
      defaultScheme: "https",
    });
    const upstreamChanged = hasOriginChanged(
      applyEnv(initialUpstreamUrl, env),
      applyEnv(state.draft.upstream_url ?? "", env),
      { defaultScheme: "https" },
    );
    const tokenChanged = hasOriginChanged(
      applyEnv(initialTokenUrl, env),
      applyEnv(state.configForm.auth_flow.token_url ?? "", env),
      {
        defaultScheme: "https",
        fallbackInitialOrigin: baseOrigin || undefined,
      },
    );
    const epOriginChanged = state.endpoints.some((ep, idx) => {
      const key = ep.id || `ep-${idx + 1}`;
      const initialBase = initialEndpointBaseUrls[key] ?? "";
      return hasOriginChanged(
        applyEnv(initialBase, env),
        applyEnv(ep.base_url ?? "", env),
        {
          defaultScheme: "https",
          fallbackInitialOrigin: baseOrigin || undefined,
        },
      );
    });
    if (!upstreamChanged && !tokenChanged && !epOriginChanged) return;

    const clonedCreds = state.configForm.auth_flow.credentials.map((c) => ({
      ...c,
    }));
    const clonedHeaders = state.configForm.auth_flow.headers.map((h) => ({
      ...h,
    }));
    const cleared = clearMaskedKvEntries(clonedCreds);
    const clearedHeaders = clearMaskedKvEntries(clonedHeaders);
    if (cleared.length === 0 && clearedHeaders.length === 0) return;

    set((prev) => ({
      configForm: {
        ...prev.configForm,
        auth_flow: {
          ...prev.configForm.auth_flow,
          credentials: clonedCreds,
          headers: clonedHeaders,
        },
      },
      originClearedCredKeys:
        cleared.length > 0
          ? Array.from(new Set([...prev.originClearedCredKeys, ...cleared]))
          : prev.originClearedCredKeys,
    }));
  }

  function requestConfirm(opts: {
    title: string;
    description: string;
    confirmLabel?: string;
    danger?: boolean;
  }): Promise<boolean> {
    const existing = get().confirmState;
    if (existing?.open) {
      existing.resolve(false);
    }
    return new Promise<boolean>((resolve) => {
      set({
        confirmState: {
          open: true,
          title: opts.title,
          description: opts.description,
          confirmLabel: opts.confirmLabel,
          danger: opts.danger,
          resolve,
        },
      });
    });
  }

  function blockIfOriginCredsMissing(): boolean {
    const state = get();
    if (!selectRequiredCredKeys(state).length) return false;
    get().setReqTab("auth");
    showWarningToast($t("upstreamConnector.originChangedCredRequired"));
    return true;
  }

  function blockIfUnresolved(names: string[]): boolean {
    if (!names.length) return false;
    showWarningToast(
      $t("upstreamConnector.unresolvedVarsBlock", { vars: names.join(", ") }),
    );
    return true;
  }

  function cryptoIssuesOnEndpoints(): string[] {
    const msgs: string[] = [];
    for (const ep of get().endpoints) {
      if (!ep.crypto) continue;
      const issues = validateCryptoProfileClient(parseCryptoProfile(ep.crypto));
      for (const issue of issues) {
        msgs.push(
          `${ep.name || ep.path || ep.id || "endpoint"}: ${issue.message}`,
        );
      }
    }
    return msgs;
  }

  function tlsIssuesAll(): string[] {
    const state = get();
    const flushed = flushActiveBuffersIntoEndpoints(state);
    set({ endpoints: flushed });
    const msgs: string[] = [];
    for (const code of validateTlsFormClient(state.supplierTlsForm)) {
      msgs.push(
        `${$t("upstreamConnector.tlsSupplierSection")}: ${$t(`upstreamConnector.tlsErr.${code}`)}`,
      );
    }
    for (const ep of flushed) {
      if (!ep.tls) continue;
      const form = tlsSpecToForm(ep.tls);
      for (const code of validateTlsFormClient(form)) {
        msgs.push(
          `${ep.name || ep.path || ep.id || "endpoint"}: ${$t(`upstreamConnector.tlsErr.${code}`)}`,
        );
      }
    }
    return msgs;
  }

  function beginTabRun(epId: string): { signal: AbortSignal; gen: number } {
    tabAbortControllers.get(epId)?.abort();
    const ac = new AbortController();
    tabAbortControllers.set(epId, ac);
    let nextGen = 1;
    set((prev) => {
      const cur = prev.tabRuntimeMap[epId] ?? createTabRuntime();
      nextGen = cur.runGen + 1;
      const nextLayout = {
        activeSidebarPanel: prev.activeSidebarPanel,
        sidebarCollapsed: prev.sidebarCollapsed,
        responseCollapsed: false,
        responseSplitDir: prev.responseSplitDir,
      };
      persistIdeLayout(nextLayout);
      return {
        responseCollapsed: false,
        tabRuntimeMap: {
          ...prev.tabRuntimeMap,
          [epId]: {
            ...cur,
            running: true,
            runGen: nextGen,
          },
        },
      };
    });
    return { signal: ac.signal, gen: nextGen };
  }

  function finishTabRun(epId: string, gen: number): void {
    const cur = get().tabRuntimeMap[epId];
    if (!cur || cur.runGen !== gen) return;
    tabAbortControllers.delete(epId);
    set((prev) => {
      const latest = prev.tabRuntimeMap[epId];
      if (!latest || latest.runGen !== gen) return prev;
      return {
        tabRuntimeMap: {
          ...prev.tabRuntimeMap,
          [epId]: {
            ...latest,
            running: false,
          },
        },
      };
    });
  }

  async function refreshGate(): Promise<void> {
    const curDraft = get().draft;
    if (!curDraft || !isPersistedDraftId(curDraft.id)) return;
    try {
      const d = await getDraft(curDraft.id);
      set((prev) =>
        prev.draft && prev.draft.id === curDraft.id
          ? { draft: { ...prev.draft, gate: d.gate } }
          : prev,
      );
    } catch {
      // Ignore transient refresh failure
    }
  }

  async function runLatestSession<T>(
    task: (signal: AbortSignal, isLatest: () => boolean) => Promise<T>,
  ): Promise<T | undefined> {
    sessionAbortController?.abort();
    const ac = new AbortController();
    sessionAbortController = ac;
    const seq = ++sessionSeq;
    const isLatest = () => seq === sessionSeq && !ac.signal.aborted;
    set({ bootstrapping: true });
    try {
      return await task(ac.signal, isLatest);
    } finally {
      if (seq === sessionSeq) {
        set({ bootstrapping: false });
      }
    }
  }

  async function openSession(
    rawDraft: ConnectorDraft,
    prefetchedSupplier?: Supplier,
    opts?: {
      signal?: AbortSignal;
      isLatest?: () => boolean;
      navigate?: WorkbenchNavigate;
      isEditMode?: boolean;
    },
  ): Promise<boolean> {
    const isLatest = opts?.isLatest ?? (() => true);
    if (!isLatest()) return false;
    if (isMicroserviceUpstream(rawDraft) && rawDraft.linked_supplier_id) {
      set({ draft: null });
      leaveForMicroservice(rawDraft.linked_supplier_id, opts?.navigate);
      return false;
    }
    const coerced = isMicroserviceUpstream(rawDraft);
    const d = coerced ? asCodeFreeDraft(rawDraft) : rawDraft;

    get().abortAllRuns();
    ephemeralRunIds = [];

    const normalizedDraft: ConnectorDraft = {
      ...d,
      upstream_type: CODE_FREE_UPSTREAM_TYPE,
      endpoints: Array.isArray(d.endpoints) ? d.endpoints : [],
      config: { ...(d.config || {}), upstream_type: CODE_FREE_UPSTREAM_TYPE },
    };
    const { configForm, supplierTimeoutMs, supplierTlsForm } = parseApiConfig(
      (normalizedDraft.config || {}) as Record<string, unknown>,
    );
    const normalizedEndpoints = (normalizedDraft.endpoints || []).map(
      (ep, i) => normalizeWorkbenchEndpoint(ep, i),
    );
    const firstEp = normalizedEndpoints[0] ?? null;
    const firstId = firstEp?.id || "";

    const nextPublishForm: PublishFormState = {
      ...get().publishForm,
      supplier_code: "",
      supplier_name: normalizedDraft.name,
    };
    let resolvedDraftName = normalizedDraft.name;

    if (d.linked_supplier_id) {
      try {
        const sup =
          prefetchedSupplier?.id === d.linked_supplier_id
            ? prefetchedSupplier
            : await fetchSupplierById(d.linked_supplier_id, opts?.signal);
        if (!isLatest()) return false;
        nextPublishForm.supplier_code = sup.supplier_code;
        nextPublishForm.supplier_name = sup.name;
        resolvedDraftName = sup.name;
      } catch {
        if (!isLatest()) return false;
        nextPublishForm.supplier_code = d.draft_code
          .replace(/^from-/i, "")
          .replace(/-[a-f0-9]{6}$/i, "")
          .toUpperCase();
        nextPublishForm.supplier_name = d.name;
      }
    } else {
      nextPublishForm.supplier_code =
        d.name
          .replace(/[^A-Za-z0-9_-]/g, "-")
          .replace(/-+/g, "-")
          .replace(/^-|-$/g, "")
          .toUpperCase()
          .slice(0, 32) || genSessionCode().toUpperCase();
      nextPublishForm.supplier_name = d.name;
    }

    normalizedDraft.name = resolvedDraftName;
    const envState = normalizeWorkbenchEnvDoc(d.workbench_env ?? []);
    const kvBuffers = kvBuffersForEndpoint(firstEp);
    const urlDraft = composeUrlForState(
      normalizedDraft,
      firstEp,
      kvBuffers.queryRows,
    );

    let loadedRuns: ConnectorRun[] = [];
    if (isPersistedDraftId(d.id)) {
      try {
        loadedRuns =
          (await listRuns(
            d.id,
            opts?.signal ? { signal: opts.signal } : undefined,
          )) || [];
        if (!isLatest()) return false;
      } catch {
        if (!isLatest()) return false;
        loadedRuns = [];
      }
    }

    const nextTabRuntimeMap: Record<string, EndpointTabRuntime> = {};
    for (const ep of normalizedEndpoints) {
      if (!ep.id) continue;
      const hit = loadedRuns.find(
        (r) =>
          r.endpoint_key &&
          (r.endpoint_key === ep.id ||
            r.endpoint_key === ep.name ||
            r.endpoint_key === ep.path),
      );
      nextTabRuntimeMap[ep.id] = {
        ...createTabRuntime(),
        lastRun: hit ?? null,
      };
    }
    if (firstId && nextTabRuntimeMap[firstId] && !nextTabRuntimeMap[firstId]!.lastRun && loadedRuns[0]) {
      nextTabRuntimeMap[firstId] = {
        ...nextTabRuntimeMap[firstId]!,
        lastRun: loadedRuns[0],
      };
    }

    const nextCaptured: Record<string, string> = {};
    for (const r of [...loadedRuns].reverse()) {
      const cap = r.response_preview?.captured;
      if (cap && typeof cap === "object") {
        Object.assign(nextCaptured, cap);
      }
    }

    captureOriginBaselineFrom(
      normalizedDraft,
      configForm,
      normalizedEndpoints,
    );

    set({
      draft: normalizedDraft,
      isEditMode: opts?.isEditMode ?? Boolean(normalizedDraft.linked_supplier_id),
      configForm,
      supplierTimeoutMs,
      supplierTlsForm,
      endpoints: normalizedEndpoints,
      openTabIds: firstId ? [firstId] : [],
      activeTabId: firstId,
      tabRuntimeMap: nextTabRuntimeMap,
      runs: loadedRuns,
      envState,
      capturedVars: nextCaptured,
      ...kvBuffers,
      urlDraft,
      urlFocused: false,
      urlMissing: false,
      originClearedCredKeys: [],
      publishForm: nextPublishForm,
      publishResult: null,
      saveError: false,
      lastSavedAt: isPersistedDraftId(d.id)
        ? Date.parse(d.updated_at) || Date.now()
        : null,
    });

    const latestState = get();
    const passedStaticSigMap = buildPassedGateSigMap(latestState, d.gate);
    const savedSnapshotSig = coerced ? "" : computeSnapshotSig(latestState);
    set({ passedStaticSigMap, savedSnapshotSig });

    syncRouteForDraft(normalizedDraft, opts?.navigate);
    void get().refreshSwitcherOptions();
    return true;
  }

  function applyRequestUrlToState(raw: string): void {
    const state = get();
    const ep = selectActiveEp(state);
    const idx = selectActiveEpIdx(state);
    if (!state.draft || !ep || idx < 0) return;
    const parts = trySplitRequestUrl(raw);
    if (!parts) return;

    let nextDraft = state.draft;
    const nextEp: ConnectorEndpoint = { ...ep, path: parts.path };
    if (parts.base) {
      if ((ep.base_url || "").trim()) {
        nextEp.base_url = parts.base;
      } else {
        nextDraft = { ...state.draft, upstream_url: parts.base };
      }
    }
    const nextEndpoints = [...state.endpoints];
    nextEndpoints[idx] = nextEp;
    const nextQueryRows = mergeQueryFromMap(parts.query, state.queryRows);
    set({
      draft: nextDraft,
      endpoints: nextEndpoints,
      queryRows: nextQueryRows,
    });
    runOriginPinningCheck();
  }

  function localSendBlock(focusUrl?: () => void): boolean {
    const state = get();
    const ep = selectActiveEp(state);
    const url = (
      state.urlDraft ||
      ep?.base_url ||
      state.draft?.upstream_url ||
      ""
    ).trim();
    if (!url) {
      set({ urlMissing: true, responseCollapsed: false });
      showWarningToast($t("upstreamConnector.urlRequired"));
      focusUrl?.();
      return true;
    }
    set({ urlMissing: false });
    const method = (ep?.method || "GET").trim();
    if (!isLegalHttpMethod(method)) {
      showWarningToast($t("upstreamConnector.methodInvalid"));
      return true;
    }
    return false;
  }

  function appliedRequest() {
    const flushed = flushActiveBuffersIntoEndpoints(get());
    set({ endpoints: flushed });
    const state = get();
    const ep = selectActiveEp(state);
    if (!ep) return null;
    const env = selectRuntimeEnvMap(state);
    const path = applyEnv(ep.path || "/", env);
    const headers: Record<string, string> = {};
    Object.entries(ep.headers || {}).forEach(([k, v]) => {
      headers[applyEnv(k, env)] = applyEnv(String(v), env);
    });
    const query: Record<string, string> = {};
    Object.entries(ep.query || {}).forEach(([k, v]) => {
      query[applyEnv(k, env)] = applyEnv(String(v), env);
    });
    let body: unknown = ep.body;
    if (defaultBodyContentType(ep) === "none") {
      body = undefined;
    } else if (typeof body === "string") {
      body = applyEnv(body, env);
    } else if (body != null) {
      try {
        body = JSON.parse(applyEnv(JSON.stringify(body), env));
      } catch {
        // Keep raw object if interpolation yields invalid JSON
      }
    }
    return { method: ep.method || "GET", path, headers, query, body };
  }

  return {
    ...createInitialState(),

    setSidebarPanel(panel) {
      if (panel === "env") {
        set({ envDrawerOpen: true });
        return;
      }
      set((prev) => {
        const next = { ...prev, activeSidebarPanel: panel };
        persistIdeLayout(next);
        return { activeSidebarPanel: panel };
      });
    },

    setSidebarCollapsed(collapsed) {
      set((prev) => {
        const next = { ...prev, sidebarCollapsed: collapsed };
        persistIdeLayout(next);
        return { sidebarCollapsed: collapsed };
      });
    },

    toggleSidebar() {
      get().setSidebarCollapsed(!get().sidebarCollapsed);
    },

    openSidebarTo(panel) {
      if (panel === "env") {
        set({ envDrawerOpen: true });
        return;
      }
      set((prev) => {
        const next = {
          ...prev,
          activeSidebarPanel: panel,
          sidebarCollapsed: false,
        };
        persistIdeLayout(next);
        return { activeSidebarPanel: panel, sidebarCollapsed: false };
      });
    },

    setResponseCollapsed(collapsed) {
      set((prev) => {
        const next = { ...prev, responseCollapsed: collapsed };
        persistIdeLayout(next);
        return { responseCollapsed: collapsed };
      });
    },

    toggleResponseCollapsed() {
      get().setResponseCollapsed(!get().responseCollapsed);
    },

    toggleSplitDirection() {
      set((prev) => {
        const responseSplitDir: SplitDirection =
          prev.responseSplitDir === "vertical" ? "horizontal" : "vertical";
        const next = { ...prev, responseSplitDir };
        persistIdeLayout(next);
        return { responseSplitDir };
      });
    },

    setEnvDrawerOpen(open) {
      set({ envDrawerOpen: open });
    },

    setShowImport(open) {
      set({ showImport: open });
    },

    setShowPublish(open) {
      set({ showPublish: open });
    },

    setShowCommands(open) {
      set({ showCommands: open });
    },

    setShowShortcuts(open) {
      set({ showShortcuts: open });
    },

    setImportTab(tab) {
      set({ importTab: tab });
    },

    setImportMode(mode) {
      set({ importMode: mode });
    },

    setImportCurlText(text) {
      set({ importCurlText: text });
    },

    setImportJsonText(text) {
      set({ importJsonText: text });
    },

    updatePublishForm(patch) {
      set((prev) => ({
        publishForm: { ...prev.publishForm, ...patch },
      }));
    },

    setPublishChannels(channels) {
      set({ publishChannels: channels });
    },

    resolveConfirm(ok) {
      const cur = get().confirmState;
      if (!cur) return;
      set({ confirmState: null });
      cur.resolve(ok);
    },

    setReqTab(tab) {
      const ep = selectActiveEp(get());
      const id = ep?.id;
      if (!id) return;
      set((prev) => ({
        tabRuntimeMap: {
          ...prev.tabRuntimeMap,
          [id]: {
            ...(prev.tabRuntimeMap[id] ?? createTabRuntime()),
            reqTab: tab,
          },
        },
      }));
    },

    setResTab(tab) {
      const ep = selectActiveEp(get());
      const id = ep?.id;
      if (!id) return;
      set((prev) => ({
        tabRuntimeMap: {
          ...prev.tabRuntimeMap,
          [id]: {
            ...(prev.tabRuntimeMap[id] ?? createTabRuntime()),
            resTab: tab,
          },
        },
      }));
    },

    updateDraft(patch) {
      set((prev) => {
        if (!prev.draft) return prev;
        const nextDraft = { ...prev.draft, ...patch };
        const ep = selectActiveEp(prev);
        const nextUrl = prev.urlFocused
          ? prev.urlDraft
          : composeUrlForState(nextDraft, ep, prev.queryRows);
        return {
          draft: nextDraft,
          urlDraft: nextUrl,
          urlMissing: nextUrl.trim() ? false : prev.urlMissing,
        };
      });
      if ("upstream_url" in patch) {
        runOriginPinningCheck();
      }
    },

    updateActiveEndpoint(updater) {
      const idx = selectActiveEpIdx(get());
      if (idx < 0) return;
      get().updateEndpointAt(idx, updater);
    },

    updateEndpointAt(index, updater) {
      const prevState = get();
      const target = prevState.endpoints[index];
      if (!target) return;
      const updated = updater({ ...target });
      const adapterChanged = updated.adapter !== target.adapter;
      const normalized = adapterChanged
        ? ensureEndpointAdapterFields({ ...updated })
        : updated;
      const nextEndpoints = [...prevState.endpoints];
      nextEndpoints[index] = normalized;
      const isActive = index === selectActiveEpIdx(prevState);

      const patch: Partial<WorkbenchState> = { endpoints: nextEndpoints };
      if (isActive && adapterChanged) {
        const kv = kvBuffersForEndpoint(normalized);
        patch.gqlVarRows = kv.gqlVarRows;
        patch.gqlEnumRows = kv.gqlEnumRows;
      }
      if (isActive && !prevState.urlFocused) {
        const nextUrl = composeUrlForState(
          prevState.draft,
          normalized,
          prevState.queryRows,
        );
        patch.urlDraft = nextUrl;
        if (nextUrl.trim()) patch.urlMissing = false;
      }
      set(patch);
      if (normalized.base_url !== target.base_url) {
        runOriginPinningCheck();
      }
    },

    setHeaderRows(rows) {
      set({ headerRows: rows });
    },

    setQueryRows(rows) {
      set((prev) => {
        const ep = selectActiveEp(prev);
        const nextUrl = prev.urlFocused
          ? prev.urlDraft
          : composeUrlForState(prev.draft, ep, rows);
        return {
          queryRows: rows,
          urlDraft: nextUrl,
          urlMissing: nextUrl.trim() ? false : prev.urlMissing,
        };
      });
    },

    setGqlVarRows(rows) {
      set({ gqlVarRows: rows });
    },

    setGqlEnumRows(rows) {
      set({ gqlEnumRows: rows });
    },

    setUrlFocused(focused) {
      set({ urlFocused: focused });
    },

    onUrlInput(raw) {
      set((prev) => ({
        urlDraft: raw,
        urlMissing: raw.trim() ? false : prev.urlMissing,
      }));
      applyRequestUrlToState(raw);
    },

    onUrlBlur() {
      const raw = get().urlDraft;
      applyRequestUrlToState(raw);
      const state = get();
      const ep = selectActiveEp(state);
      const composed = composeUrlForState(state.draft, ep, state.queryRows);
      set({
        urlFocused: false,
        urlDraft: composed,
        urlMissing: composed.trim() ? false : state.urlMissing,
      });
    },

    applyCurlPaste(parsed) {
      const src = parsed.endpoints[0];
      const state = get();
      const idx = selectActiveEpIdx(state);
      const ep = selectActiveEp(state);
      if (!src || !state.draft || !ep || idx < 0) return;
      const nextDraft = parsed.upstream_url
        ? { ...state.draft, upstream_url: parsed.upstream_url }
        : state.draft;
      const nextEp: ConnectorEndpoint = {
        ...ep,
        method: src.method || ep.method,
        path: src.path || "/",
        headers: src.headers || {},
        query: src.query || {},
        ...(src.body !== undefined ? { body: src.body } : {}),
      };
      const nextEndpoints = [...state.endpoints];
      nextEndpoints[idx] = nextEp;
      const headerRows = mapToKv(nextEp.headers);
      const queryRows = mapToKv(nextEp.query);
      const urlDraft = composeUrlForState(nextDraft, nextEp, queryRows);
      set({
        draft: nextDraft,
        endpoints: nextEndpoints,
        headerRows,
        queryRows,
        urlDraft,
        urlMissing: urlDraft.trim() ? false : state.urlMissing,
      });
      runOriginPinningCheck();
      showSuccessToast($t("upstreamConnector.pasteCurlOk"));
    },

    setConfigForm(updater) {
      const prevForm = get().configForm;
      const nextForm = updater(prevForm);
      const prevCleared = get().originClearedCredKeys;
      let nextCleared = prevCleared;
      if (prevCleared.length > 0) {
        const creds = nextForm.auth_flow.credentials;
        nextCleared = prevCleared.filter((k) => {
          const row = creds.find((item) => item.key.trim() === k);
          return (
            Boolean(row) &&
            (!row!.value.trim() || isMaskedSecretValue(row!.value))
          );
        });
      }
      set({ configForm: nextForm, originClearedCredKeys: nextCleared });
      if (nextForm.auth_flow.token_url !== prevForm.auth_flow.token_url) {
        runOriginPinningCheck();
      }
    },

    setSupplierTimeoutMs(ms) {
      set({ supplierTimeoutMs: ms });
    },

    setSupplierTlsForm(form) {
      set({ supplierTlsForm: form });
    },

    setEndpointTlsForm(form) {
      set({ endpointTlsForm: form });
    },

    setEpAuthProfile(profile) {
      get().updateActiveEndpoint((ep) => ({
        ...ep,
        auth: {
          ...(ep.auth || {}),
          profile: profile === "inherit" ? "none" : profile,
        },
      }));
    },

    onCryptoProfileChange(value) {
      get().updateActiveEndpoint((ep) => ({
        ...ep,
        crypto: value,
      }));
    },

    onBodyEncodingChange(value) {
      const enc =
        value === "form" || value === "none" || value === "json"
          ? value
          : "json";
      get().updateActiveEndpoint((ep) => {
        const nextEp = { ...ep };
        setBodyEncoding(nextEp, enc);
        return nextEp;
      });
    },

    onAdapterQuickSelect(adapter) {
      if (!selectCanWriteWorkbench()) return;
      get().updateActiveEndpoint((ep) => {
        const nextEp = { ...ep, adapter };
        ensureEndpointAdapterFields(nextEp);
        return nextEp;
      });
    },

    async uploadArtifactFile(file) {
      const ep = selectActiveEp(get());
      if (!ep) return;
      set({ uploadingArtifact: true });
      try {
        const content_base64 = await fileToBase64(file);
        const art = await uploadArtifact({
          artifact_type: "fds",
          name: file.name,
          content_base64,
        });
        get().updateActiveEndpoint((cur) => ({
          ...cur,
          artifact_id: art.id,
        }));
        showSuccessToast($t("upstreamConnector.artifactUploaded"));
      } finally {
        set({ uploadingArtifact: false });
      }
    },

    openEndpointTab(epId) {
      const state = get();
      if (!epId || epId === state.activeTabId) return;
      const flushedEndpoints = flushActiveBuffersIntoEndpoints(state);
      const targetEp =
        flushedEndpoints.find((ep) => ep.id === epId) ?? null;
      if (!targetEp) return;
      const nextOpenTabIds = state.openTabIds.includes(epId)
        ? state.openTabIds
        : [...state.openTabIds, epId];
      const nextTabRuntimeMap = state.tabRuntimeMap[epId]
        ? state.tabRuntimeMap
        : { ...state.tabRuntimeMap, [epId]: createTabRuntime() };
      const kv = kvBuffersForEndpoint(targetEp);
      const urlDraft = composeUrlForState(state.draft, targetEp, kv.queryRows);
      set({
        endpoints: flushedEndpoints,
        openTabIds: nextOpenTabIds,
        activeTabId: epId,
        tabRuntimeMap: nextTabRuntimeMap,
        ...kv,
        urlDraft,
        urlFocused: false,
        urlMissing: urlDraft.trim() ? false : state.urlMissing,
      });
    },

    closeEndpointTab(epId) {
      const state = get();
      const pos = state.openTabIds.indexOf(epId);
      if (pos < 0 || state.openTabIds.length <= 1) return;
      const nextTabs = state.openTabIds.filter((id) => id !== epId);
      if (state.activeTabId === epId) {
        const fallback = nextTabs[Math.max(0, pos - 1)] || nextTabs[0] || "";
        set({ openTabIds: nextTabs });
        if (fallback) get().openEndpointTab(fallback);
      } else {
        set({ openTabIds: nextTabs });
      }
    },

    switchEndpointByIndex(index) {
      const state = get();
      const ep = state.endpoints[index];
      if (!ep) return;
      let epId = ep.id;
      if (!epId) {
        epId = `ep-${index + 1}`;
        const nextEndpoints = [...state.endpoints];
        nextEndpoints[index] = { ...ep, id: epId };
        set({ endpoints: nextEndpoints });
      }
      get().openEndpointTab(epId);
    },

    addEndpoint(defaultTag = "") {
      if (!selectCanWriteWorkbench()) return;
      const state = get();
      const flushedEndpoints = flushActiveBuffersIntoEndpoints(state);
      const created = normalizeWorkbenchEndpoint(
        {
          id: `ep-${Date.now().toString(36)}`,
          name: `endpoint-${flushedEndpoints.length + 1}`,
          method: "POST",
          path: "",
          adapter: "http_json",
          body_content_type: "json",
          body_encoding: "json",
          body: {},
          wrap: "method_biz_content",
          assertions: [],
          captures: [],
          gate: true,
          ...(defaultTag.trim() ? { tag: defaultTag.trim() } : {}),
        },
        flushedEndpoints.length,
      );
      set({ endpoints: [...flushedEndpoints, created] });
      if (created.id) {
        get().openEndpointTab(created.id);
      }
    },

    duplicateEndpoint(index) {
      if (!selectCanWriteWorkbench()) return;
      const state = get();
      const flushedEndpoints = flushActiveBuffersIntoEndpoints(state);
      const src = flushedEndpoints[index];
      if (!src) return;
      const cloned: ConnectorEndpoint = normalizeWorkbenchEndpoint(
        {
          ...(JSON.parse(JSON.stringify(src)) as ConnectorEndpoint),
          id: `ep-${Date.now().toString(36)}`,
          name: `${src.name || "endpoint"}-copy`,
        },
        index + 1,
      );
      const nextEndpoints = [...flushedEndpoints];
      nextEndpoints.splice(index + 1, 0, cloned);
      set({ endpoints: nextEndpoints });
      if (cloned.id) {
        get().openEndpointTab(cloned.id);
      }
    },

    deleteEndpoint(index) {
      if (!selectCanWriteWorkbench()) return;
      const state = get();
      const flushedEndpoints = flushActiveBuffersIntoEndpoints(state);
      if (flushedEndpoints.length <= 1) {
        showWarningToast($t("upstreamConnector.keepOneEndpoint"));
        return;
      }
      const victim = flushedEndpoints[index];
      const victimId = victim?.id || "";
      const nextEndpoints = flushedEndpoints.filter((_, i) => i !== index);
      const nextRuntimeMap = { ...state.tabRuntimeMap };
      if (victimId) {
        tabAbortControllers.get(victimId)?.abort();
        tabAbortControllers.delete(victimId);
        delete nextRuntimeMap[victimId];
      }
      let nextOpenTabIds = victimId
        ? state.openTabIds.filter((id) => id !== victimId)
        : [...state.openTabIds];
      if (!nextOpenTabIds.length && nextEndpoints[0]?.id) {
        nextOpenTabIds = [nextEndpoints[0].id];
      }
      const needSwitch =
        !state.activeTabId ||
        !nextEndpoints.some((e) => e.id === state.activeTabId);
      const nextActiveId = needSwitch
        ? nextOpenTabIds[0] || nextEndpoints[0]?.id || ""
        : state.activeTabId;
      const nextActiveEp =
        nextEndpoints.find((e) => e.id === nextActiveId) ??
        nextEndpoints[0] ??
        null;
      const kv = needSwitch ? kvBuffersForEndpoint(nextActiveEp) : null;
      const nextUrl =
        needSwitch && kv
          ? composeUrlForState(state.draft, nextActiveEp, kv.queryRows)
          : state.urlDraft;
      set({
        endpoints: nextEndpoints,
        tabRuntimeMap: nextRuntimeMap,
        openTabIds: nextOpenTabIds,
        activeTabId: nextActiveId,
        ...(kv || {}),
        urlDraft: nextUrl,
      });
    },

    toggleEndpointGate(index) {
      if (!selectCanWriteWorkbench()) return;
      get().updateEndpointAt(index, (ep) => ({
        ...ep,
        gate: ep.gate === false,
      }));
    },

    setEnvState(updater) {
      set((prev) => ({ envState: updater(prev.envState) }));
    },

    setActiveEnv(id) {
      set((prev) => ({
        envState: { ...prev.envState, activeEnvId: id || null },
      }));
    },

    addNamedEnv(name) {
      if (!selectCanWriteWorkbench()) return;
      const created = createNamedEnv(name);
      set((prev) => ({
        envState: {
          ...prev.envState,
          environments: [...prev.envState.environments, created],
          activeEnvId: created.id,
        },
      }));
    },

    deleteNamedEnv(id) {
      if (!selectCanWriteWorkbench()) return;
      set((prev) => {
        const environments = prev.envState.environments.filter(
          (e) => e.id !== id,
        );
        const activeEnvId =
          prev.envState.activeEnvId === id
            ? (environments[0]?.id ?? null)
            : prev.envState.activeEnvId;
        return {
          envState: {
            ...prev.envState,
            environments,
            activeEnvId,
          },
        };
      });
    },

    exportEnvToFile() {
      if (!selectCanWriteWorkbench()) return;
      const state = get();
      const doc = exportWorkbenchEnvJson(state.envState, state.draft?.name);
      const blob = new Blob([JSON.stringify(doc, null, 2)], {
        type: "application/json",
      });
      saveAs(blob, `${state.draft?.name || "workbench"}.env.json`);
    },

    importEnvFromText(rawText) {
      if (!selectCanWriteWorkbench()) return;
      const next = importWorkbenchEnvJson(rawText, get().envState);
      set({ envState: next });
      showSuccessToast($t("upstreamConnector.envImportOk"));
    },

    clearCapturedVars() {
      set({ capturedVars: {} });
    },

    removeCapturedVar(name) {
      set((prev) => {
        const next = { ...prev.capturedVars };
        delete next[name];
        return { capturedVars: next };
      });
    },

    selectHistoryRun(run) {
      const state = get();
      const ep = state.endpoints.find(
        (e) =>
          run.endpoint_key &&
          (e.id === run.endpoint_key ||
            e.name === run.endpoint_key ||
            e.path === run.endpoint_key),
      );
      const targetId = ep?.id || selectActiveEp(state)?.id || "";
      if (ep?.id && ep.id !== state.activeTabId) {
        get().openEndpointTab(ep.id);
      }
      if (targetId) {
        set((prev) => ({
          responseCollapsed: false,
          tabRuntimeMap: {
            ...prev.tabRuntimeMap,
            [targetId]: {
              ...(prev.tabRuntimeMap[targetId] ?? createTabRuntime()),
              lastRun: run,
            },
          },
        }));
      } else {
        set({ responseCollapsed: false });
      }
    },

    async refreshSwitcherOptions() {
      set({ switcherLoading: true });
      try {
        const [draftRes, supRes] = await Promise.allSettled([
          listDrafts({ page: 1, page_size: 30 }),
          fetchSuppliers({ page: 1, page_size: 50 }),
        ]);
        const patch: Partial<WorkbenchState> = {};
        if (draftRes.status === "fulfilled") {
          patch.recentDrafts = draftRes.value.drafts || [];
        }
        if (supRes.status === "fulfilled") {
          patch.transparentSuppliers = (supRes.value.suppliers || []).filter(
            (s) => resolveUpstreamType(s) !== "microservice",
          );
        }
        set(patch);
      } finally {
        set({ switcherLoading: false });
      }
    },

    async bootstrap(params, navigate) {
      lastNavigate = navigate;
      const canWrite = selectCanWriteWorkbench();
      try {
        await runLatestSession(async (signal, isLatest) => {
          const supplierId = (params.supplierId || "").trim();
          const sessionId = (params.sessionId || "").trim();

          if (supplierId) {
            const sup = await fetchSupplierById(supplierId, signal);
            if (!isLatest()) return;
            if (resolveUpstreamType(sup) === "microservice") {
              leaveForMicroservice(supplierId, navigate);
              return;
            }
            const d = canWrite
              ? await draftFromSupplier(supplierId, { signal })
              : await previewDraftFromSupplier(supplierId, { signal });
            if (!isLatest()) return;
            await openSession(d, sup, {
              signal,
              isLatest,
              navigate,
              isEditMode: true,
            });
            return;
          }

          if (sessionId) {
            const d = await getDraft(sessionId, { signal });
            if (!isLatest()) return;
            await openSession(d, undefined, {
              signal,
              isLatest,
              navigate,
              isEditMode: Boolean(d.linked_supplier_id),
            });
            return;
          }

          if (!canWrite) {
            set({ draft: null, isEditMode: false });
            void get().refreshSwitcherOptions();
            return;
          }

          await openSession(buildLocalBlankDraft(), undefined, {
            signal,
            isLatest,
            navigate,
            isEditMode: false,
          });
        });
      } catch (e) {
        if (isRunCancelled(e)) return;
        set({ draft: null });
        showErrorToast(
          (e as Error)?.message || $t("upstreamConnector.bootstrapFailed"),
        );
      }
    },

    async createBlankSession(navigate) {
      if (!selectCanWriteWorkbench()) return;
      if (!(await get().confirmDiscardIfDirty())) return;
      if (navigate) lastNavigate = navigate;
      sessionAbortController?.abort();
      sessionSeq += 1;
      await openSession(buildLocalBlankDraft(), undefined, {
        navigate,
        isEditMode: false,
      });
    },

    async switchToDraft(draftId, navigate) {
      if (!draftId || draftId === get().draft?.id) return;
      if (!(await get().confirmDiscardIfDirty())) return;
      if (navigate) lastNavigate = navigate;
      try {
        await runLatestSession(async (signal, isLatest) => {
          const d = await getDraft(draftId, { signal });
          if (!isLatest()) return;
          await openSession(d, undefined, {
            signal,
            isLatest,
            navigate,
            isEditMode: Boolean(d.linked_supplier_id),
          });
        });
      } catch (e) {
        if (isRunCancelled(e)) return;
        showErrorToast(
          (e as Error)?.message || $t("upstreamConnector.bootstrapFailed"),
        );
      }
    },

    async switchToSupplier(supplierId, navigate) {
      if (!supplierId || supplierId === get().draft?.linked_supplier_id) return;
      if (!(await get().confirmDiscardIfDirty())) return;
      if (navigate) lastNavigate = navigate;
      const canWrite = selectCanWriteWorkbench();
      try {
        await runLatestSession(async (signal, isLatest) => {
          const sup = await fetchSupplierById(supplierId, signal);
          if (!isLatest()) return;
          if (resolveUpstreamType(sup) === "microservice") {
            leaveForMicroservice(supplierId, navigate);
            return;
          }
          const d = canWrite
            ? await draftFromSupplier(supplierId, { signal })
            : await previewDraftFromSupplier(supplierId, { signal });
          if (!isLatest()) return;
          await openSession(d, sup, {
            signal,
            isLatest,
            navigate,
            isEditMode: true,
          });
        });
      } catch (e) {
        if (isRunCancelled(e)) return;
        showErrorToast(
          (e as Error)?.message || $t("upstreamConnector.bootstrapFailed"),
        );
      }
    },

    async saveDraft(opts) {
      if (opts?.navigate) lastNavigate = opts.navigate;
      if (!selectCanWriteWorkbench() || !get().draft) return;
      if (blockIfOriginCredsMissing()) {
        set({ saveError: true });
        throw new Error($t("upstreamConnector.originChangedCredRequired"));
      }
      const tlsMsgs = tlsIssuesAll();
      if (tlsMsgs.length) {
        if (!opts?.silent) showWarningToast(tlsMsgs[0]!);
        set({ saveError: true });
        throw new Error(tlsMsgs[0]);
      }
      set({ saving: true });
      try {
        const flushedEndpoints = flushActiveBuffersIntoEndpoints(get());
        set({ endpoints: flushedEndpoints });
        const curState = get();
        const curDraft = curState.draft!;
        const body = buildDraftSnapshotFromState(curState);
        const reqOpts = { showErrorMessage: !opts?.silent };

        const updated = isPersistedDraftId(curDraft.id)
          ? await updateDraft(curDraft.id, body, reqOpts)
          : await createDraft(
              {
                draft_code: curDraft.draft_code || genSessionCode(),
                name: body.name || $t("upstreamConnector.untitledName"),
                upstream_type: CODE_FREE_UPSTREAM_TYPE,
                upstream_url: body.upstream_url,
                config: body.config,
                endpoints: body.endpoints,
                workbench_env: body.workbench_env,
                source: "manual",
                claim_run_ids: ephemeralRunIds,
              },
              reqOpts,
            );

        ephemeralRunIds = [];
        const latestEndpoints = [...get().endpoints];
        if (Array.isArray(updated.endpoints)) {
          updated.endpoints.forEach((srvEp, i) => {
            if (latestEndpoints[i]) {
              latestEndpoints[i] = {
                ...latestEndpoints[i]!,
                tls: srvEp.tls ?? undefined,
              };
            }
          });
        }
        const mergedDraft: ConnectorDraft = {
          ...curDraft,
          ...updated,
          endpoints: latestEndpoints,
          upstream_type: CODE_FREE_UPSTREAM_TYPE,
        };
        const parsedCfg = updated.config
          ? parseApiConfig(updated.config as Record<string, unknown>)
          : null;
        const activeEp = selectActiveEp({
          endpoints: latestEndpoints,
          activeTabId: get().activeTabId,
        });

        set((prev) => ({
          draft: mergedDraft,
          endpoints: latestEndpoints,
          ...(parsedCfg || {}),
          endpointTlsForm: tlsSpecToForm(activeEp?.tls ?? null),
          originClearedCredKeys: [],
          saveError: false,
          lastSavedAt: Date.now(),
        }));

        const afterState = get();
        captureOriginBaselineFrom(
          afterState.draft,
          afterState.configForm,
          afterState.endpoints,
        );
        const passedStaticSigMap = buildPassedGateSigMap(
          afterState,
          updated.gate,
        );
        const savedSnapshotSig = computeSnapshotSig(afterState);
        set({ passedStaticSigMap, savedSnapshotSig });

        if (!opts?.skipRouteSync) {
          syncRouteForDraft(mergedDraft, opts?.navigate);
        }
        void get().refreshSwitcherOptions();
        if (!opts?.silent) {
          showSuccessToast($t("common.feedback.saveSuccess"));
        }
      } catch (err) {
        if (!isMfaStepUpCancelled(err)) {
          set({ saveError: true });
        }
        throw err;
      } finally {
        set({ saving: false });
      }
    },

    async saveDraftExplicit(navigate) {
      if (!selectCanWriteWorkbench() || !get().draft) return;
      try {
        await get().saveDraft({ silent: false, navigate });
      } catch {
        // Error feedback already handled in saveDraft
      }
    },

    async resyncFromLiveSupplier(navigate) {
      const targetSupplierId = get().draft?.linked_supplier_id;
      if (!selectCanWriteWorkbench() || !targetSupplierId) return;
      if (navigate) lastNavigate = navigate;
      const confirmed = await requestConfirm({
        title: $t("upstreamConnector.resyncFromLive"),
        description: $t("upstreamConnector.resyncFromLiveConfirm"),
        confirmLabel: $t("common.actions.confirm"),
        danger: false,
      });
      if (!confirmed) return;
      try {
        const done = await runLatestSession(async (signal, isLatest) => {
          const d = await draftFromSupplier(targetSupplierId, {
            force_resync: true,
            signal,
          });
          if (!isLatest()) return false;
          return await openSession(d, undefined, {
            signal,
            isLatest,
            navigate,
            isEditMode: true,
          });
        });
        if (done) {
          showSuccessToast($t("upstreamConnector.resyncOk"));
        }
      } catch (e) {
        if (isRunCancelled(e)) return;
        showErrorToast(
          isHttpError(e)
            ? e.message
            : $t("upstreamConnector.bootstrapFailed"),
        );
      }
    },

    async doRun(mode, focusUrl) {
      const state = get();
      const ep = selectActiveEp(state);
      if (!state.draft || !ep || !ep.id) return;
      if (localSendBlock(focusUrl)) return;
      if (blockIfOriginCredsMissing()) return;
      if (blockIfUnresolved(selectUnresolvedRun(get()))) return;

      const method = (ep.method || "GET").toUpperCase();
      if (mode === "business" && WRITE_METHODS.has(method)) {
        const ok = await requestConfirm({
          title: $t("upstreamConnector.writeConfirmTitle"),
          description: $t("upstreamConnector.writeConfirmMsg"),
          confirmLabel: $t("common.actions.confirm"),
          danger: false,
        });
        if (!ok) return;
      }

      const tlsMsgs = tlsIssuesAll();
      if (tlsMsgs.length) {
        showWarningToast(tlsMsgs[0]!);
        return;
      }

      const epId = ep.id;
      const { signal, gen } = beginTabRun(epId);
      try {
        if (!appliedRequest()) return;
        const curState = get();
        const curDraft = curState.draft;
        const curEp = selectActiveEp(curState);
        if (!curDraft || !curEp) return;
        const persistedId = isPersistedDraftId(curDraft.id)
          ? curDraft.id
          : null;
        const r = await runDraft(
          persistedId,
          {
            mode,
            endpoint_id: epId,
            assertions: curEp.assertions as AssertionSpec[],
            env_id: curState.envState.activeEnvId,
            captures: curState.capturedVars,
            draft_snapshot: buildDraftSnapshotFromState(curState),
          },
          { signal, ...RUN_HTTP },
        );

        if (!persistedId && r.id && !ephemeralRunIds.includes(r.id)) {
          ephemeralRunIds = [...ephemeralRunIds, r.id];
        }

        set((prev) => {
          const nextCaptured = r.response_preview?.captured
            ? { ...prev.capturedVars, ...r.response_preview.captured }
            : prev.capturedVars;
          const nextDraft =
            Array.isArray(r.gate) && prev.draft
              ? { ...prev.draft, gate: r.gate }
              : prev.draft;
          return {
            draft: nextDraft,
            capturedVars: nextCaptured,
            runs: [r, ...prev.runs].slice(0, 50),
            tabRuntimeMap: {
              ...prev.tabRuntimeMap,
              [epId]: {
                ...(prev.tabRuntimeMap[epId] ?? createTabRuntime()),
                lastRun: r,
              },
            },
          };
        });

        if (Array.isArray(r.gate)) {
          const nextMap = buildPassedGateSigMap(get(), r.gate);
          set({ passedStaticSigMap: nextMap });
        } else if (persistedId) {
          await refreshGate();
        }

        if (r.ok) {
          showSuccessToast($t("upstreamConnector.runOk"));
        } else {
          showErrorToast(
            r.error_message || r.error_code || $t("upstreamConnector.runFail"),
          );
        }
      } catch (err) {
        if (isRunCancelled(err)) {
          showInfoToast($t("upstreamConnector.runCancelled"));
          return;
        }
        showErrorToast(
          isHttpError(err) ? err.message : $t("upstreamConnector.runFail"),
        );
      } finally {
        finishTabRun(epId, gen);
      }
    },

    async doLiveRun() {
      const state = get();
      const ep = selectActiveEp(state);
      const supplierId = state.draft?.linked_supplier_id;
      if (!supplierId || !ep || !ep.id) return;
      const flushed = flushActiveBuffersIntoEndpoints(state);
      set({ endpoints: flushed });
      const epId = ep.id;
      const channelCode = autoChannelsFromEndpoints(flushed).find(
        (c) => c.endpoint_id === epId,
      )?.channel_code;
      if (!channelCode) return;

      const { signal, gen } = beginTabRun(epId);
      try {
        const latestEp = selectActiveEp(get());
        const r = await liveRun(
          supplierId,
          { channel_code: channelCode, payload: latestEp?.body ?? {} },
          { signal, ...RUN_HTTP },
        );
        set((prev) => ({
          runs: [r, ...prev.runs].slice(0, 50),
          tabRuntimeMap: {
            ...prev.tabRuntimeMap,
            [epId]: {
              ...(prev.tabRuntimeMap[epId] ?? createTabRuntime()),
              lastRun: r,
            },
          },
        }));
        if (r.ok) {
          showSuccessToast($t("upstreamConnector.runOk"));
        } else {
          showErrorToast(
            r.error_message || r.error_code || $t("upstreamConnector.runFail"),
          );
        }
      } catch (err) {
        if (isRunCancelled(err)) {
          showInfoToast($t("upstreamConnector.runCancelled"));
          return;
        }
        showErrorToast(
          isHttpError(err) ? err.message : $t("upstreamConnector.runFail"),
        );
      } finally {
        finishTabRun(epId, gen);
      }
    },

    async doTransformPreview() {
      const state = get();
      const ep = selectActiveEp(state);
      if (!state.draft || !ep || !ep.id) return;
      if (blockIfOriginCredsMissing()) return;
      if (blockIfUnresolved(selectUnresolvedRun(get()))) return;
      const tlsMsgs = tlsIssuesAll();
      if (tlsMsgs.length) {
        showWarningToast(tlsMsgs[0]!);
        return;
      }

      const epId = ep.id;
      const { signal, gen } = beginTabRun(epId);
      try {
        const flushed = flushActiveBuffersIntoEndpoints(get());
        set({ endpoints: flushed });
        const curState = get();
        const curDraft = curState.draft!;
        const persistedId = isPersistedDraftId(curDraft.id)
          ? curDraft.id
          : null;
        const r = await transformPreview(
          persistedId,
          {
            endpoint_id: epId,
            env_id: curState.envState.activeEnvId,
            captures: curState.capturedVars,
            draft_snapshot: buildDraftSnapshotFromState(curState),
          },
          { signal, ...RUN_HTTP },
        );
        const previewRun: ConnectorRun = {
          id: "preview",
          mode: "transform_preview",
          ok: r.ok,
          latency_ms: 0,
          stages: r.stages || [],
          assertions: [],
          created_at: new Date().toISOString(),
        };
        set((prev) => ({
          tabRuntimeMap: {
            ...prev.tabRuntimeMap,
            [epId]: {
              ...(prev.tabRuntimeMap[epId] ?? createTabRuntime()),
              lastRun: previewRun,
              resTab: "pipeline",
            },
          },
        }));
      } catch (err) {
        if (isRunCancelled(err)) {
          showInfoToast($t("upstreamConnector.runCancelled"));
          return;
        }
        const message = isHttpError(err)
          ? err.message
          : $t("upstreamConnector.runFail");
        const errorRun: ConnectorRun = {
          id: "preview-error",
          mode: "transform_preview",
          ok: false,
          latency_ms: 0,
          stages: [],
          assertions: [],
          error_code: "TRANSFORM_PREVIEW_FAILED",
          error_message: message,
          created_at: new Date().toISOString(),
        };
        set((prev) => ({
          tabRuntimeMap: {
            ...prev.tabRuntimeMap,
            [epId]: {
              ...(prev.tabRuntimeMap[epId] ?? createTabRuntime()),
              lastRun: errorRun,
            },
          },
        }));
        showErrorToast(message);
      } finally {
        finishTabRun(epId, gen);
      }
    },

    abortActiveRun() {
      const id = selectActiveEp(get())?.id;
      if (!id) return;
      tabAbortControllers.get(id)?.abort();
    },

    abortAllRuns() {
      for (const ac of tabAbortControllers.values()) {
        ac.abort();
      }
      tabAbortControllers.clear();
    },

    async copyCurl() {
      const req = appliedRequest();
      const state = get();
      if (!req || !state.draft) return;
      const ep = selectActiveEp(state);
      const base = (ep?.base_url || "").trim() || state.draft.upstream_url;
      const text = buildCurl({
        baseUrl: applyEnv(base, selectRuntimeEnvMap(state)),
        method: req.method,
        path: req.path,
        headers: req.headers,
        query: req.query,
        body: req.body,
      });
      await copyWithFeedback(text);
    },

    async doImport(navigate) {
      if (!selectCanWriteWorkbench()) return;
      if (navigate) lastNavigate = navigate;
      set({ importing: true });
      try {
        const state = get();
        let parsed: ParsedImport;
        if (state.importTab === "curl") {
          parsed = parseCurl(state.importCurlText);
        } else if (state.importTab === "postman") {
          parsed = parsePostmanCollection(JSON.parse(state.importJsonText));
        } else {
          parsed = parseOpenApiDocument(JSON.parse(state.importJsonText));
        }

        if (state.importMode === "merge" && state.draft) {
          const mergedEndpoints = mergeImportedEndpoints(
            state.endpoints,
            parsed.endpoints,
          ).map((ep, i) => normalizeWorkbenchEndpoint(ep, i));
          const nextDraft: ConnectorDraft = {
            ...state.draft,
            ...(parsed.upstream_url && !state.draft.upstream_url.trim()
              ? { upstream_url: parsed.upstream_url }
              : {}),
            ...(parsed.name &&
            state.draft.name === $t("upstreamConnector.untitledName")
              ? { name: parsed.name }
              : {}),
          };
          set({
            draft: nextDraft,
            endpoints: mergedEndpoints,
            showImport: false,
          });
          showSuccessToast(
            $t("upstreamConnector.importedMerge", {
              n: parsed.endpoints.length,
            }),
          );
          return;
        }

        if (!(await get().confirmDiscardIfDirty())) {
          return;
        }
        const blank = buildLocalBlankDraft();
        set({ showImport: false });
        await openSession(blank, undefined, { navigate, isEditMode: false });

        const curDraft = get().draft;
        if (curDraft) {
          const importedEndpoints = (parsed.endpoints || []).map((ep, i) =>
            normalizeWorkbenchEndpoint(ep as ConnectorEndpoint, i),
          );
          const firstEp = importedEndpoints[0] ?? null;
          const firstId = firstEp?.id || "";
          const nextDraft: ConnectorDraft = {
            ...curDraft,
            name: parsed.name || "Imported",
            upstream_url: parsed.upstream_url || "",
          };
          const kv = kvBuffersForEndpoint(firstEp);
          const urlDraft = composeUrlForState(nextDraft, firstEp, kv.queryRows);
          set({
            draft: nextDraft,
            endpoints: importedEndpoints,
            openTabIds: firstId ? [firstId] : [],
            activeTabId: firstId,
            ...kv,
            urlDraft,
          });
        }
        showSuccessToast($t("upstreamConnector.imported"));
      } catch (e) {
        showErrorToast((e as Error)?.message || "Import failed");
      } finally {
        set({ importing: false });
      }
    },

    async doExport(navigate) {
      if (!selectCanWriteWorkbench() || !get().draft) return;
      try {
        await get().saveDraft({ silent: true, navigate });
      } catch (err) {
        if (isMfaStepUpCancelled(err)) return;
        showErrorToast($t("upstreamConnector.saveDraftFailed"));
        return;
      }
      const curDraft = get().draft;
      if (!curDraft) return;
      const col = await exportPostman(curDraft.id);
      const blob = new Blob([JSON.stringify(col, null, 2)], {
        type: "application/json",
      });
      saveAs(blob, `${curDraft.name || "connector"}.postman.json`);
    },

    async openPublish(navigate) {
      if (!selectCanWriteWorkbench()) return;
      if (blockIfOriginCredsMissing()) return;
      if (blockIfUnresolved(selectUnresolvedAll(get()))) return;

      const cryptoMsgs = cryptoIssuesOnEndpoints();
      if (cryptoMsgs.length) {
        showWarningToast(cryptoMsgs[0]!);
        return;
      }

      set((prev) => ({
        publishResult: null,
        publishChannels: autoChannelsFromEndpoints(prev.endpoints),
        publishForm: prev.draft?.name
          ? { ...prev.publishForm, supplier_name: prev.draft.name }
          : prev.publishForm,
      }));

      try {
        await get().saveDraft({ silent: true, navigate });
      } catch (err) {
        if (isMfaStepUpCancelled(err)) return;
        showErrorToast($t("upstreamConnector.saveDraftFailed"));
        return;
      }
      set({ showPublish: true });
    },

    async doPublish(navigate) {
      const state = get();
      if (!selectCanWriteWorkbench() || !state.draft) return;
      if (blockIfOriginCredsMissing()) return;
      if (blockIfUnresolved(selectUnresolvedAll(get()))) return;
      const cryptoMsgs = cryptoIssuesOnEndpoints();
      if (cryptoMsgs.length) {
        showWarningToast(cryptoMsgs[0]!);
        return;
      }
      if (!state.draft.name.trim() || !state.draft.upstream_url.trim()) {
        showWarningToast($t("upstreamConnector.fieldsRequired"));
        return;
      }
      if (
        !state.publishForm.supplier_code.trim() ||
        !state.publishForm.supplier_name.trim()
      ) {
        showWarningToast($t("upstreamConnector.publishFieldsRequired"));
        return;
      }

      set({ publishing: true, publishResult: null });
      try {
        try {
          await get().saveDraft({ silent: true, navigate });
        } catch (err) {
          if (isMfaStepUpCancelled(err)) return;
          showErrorToast($t("upstreamConnector.saveDraftFailed"));
          return;
        }
        const curState = get();
        const curDraft = curState.draft!;
        const pf = curState.publishForm;
        const r = await publishDraft(curDraft.id, {
          mode: pf.mode,
          supplier: {
            supplier_code: pf.supplier_code,
            name: pf.supplier_name,
            status: "active",
          },
          channels:
            pf.mode === "supplier_only"
              ? undefined
              : curState.publishChannels.map((c) => ({
                  endpoint_id: c.endpoint_id,
                  channel_code: c.channel_code,
                  name: c.name,
                  path_prefix: c.path_prefix,
                  cost_price: c.cost_price,
                  qps_limit: c.qps_limit,
                })),
          route:
            pf.mode === "full_onboarding"
              ? {
                  product_ref: { product_id: pf.product_id },
                  priority: 100,
                  weight: 100,
                  version_tag: "v1.0",
                }
              : undefined,
          options: {
            require_gate: pf.require_gate,
            force: {
              force_publish: pf.force_publish,
              force_reason: pf.force_reason || null,
            },
            fail_if_route_exists: true,
          },
          comment: pf.comment,
        });

        set({ publishResult: r });
        showSuccessToast($t("upstreamConnector.publishOk"));
        if (r.supplier_id) {
          set((prev) =>
            prev.draft
              ? {
                  draft: { ...prev.draft, linked_supplier_id: r.supplier_id },
                  isEditMode: true,
                }
              : prev,
          );
          const updatedDraft = get().draft;
          if (updatedDraft) {
            syncRouteForDraft(updatedDraft, navigate);
          }
        }
      } catch (err) {
        if (isMfaStepUpCancelled(err)) return;
        if (!isHttpError(err)) {
          showErrorToast(
            err instanceof Error && err.message ? err.message : String(err),
          );
        }
      } finally {
        set({ publishing: false });
      }
    },

    async confirmDiscardIfDirty() {
      const state = get();
      if (!selectDirty(state)) return true;
      return await requestConfirm({
        title: $t("upstreamConnector.leaveUnsavedTitle"),
        description: $t("upstreamConnector.leaveUnsavedMessage"),
        confirmLabel: $t("upstreamConnector.leaveConfirmExit"),
        danger: true,
      });
    },

    resetStore() {
      get().abortAllRuns();
      sessionAbortController?.abort();
      sessionAbortController = null;
      sessionSeq += 1;
      initialUpstreamUrl = "";
      initialTokenUrl = "";
      initialEndpointBaseUrls = {};
      ephemeralRunIds = [];
      const curConfirm = get().confirmState;
      if (curConfirm?.open) {
        curConfirm.resolve(false);
      }
      set(createInitialState());
    },
  };
});
