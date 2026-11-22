import { watchDebounced } from "@vueuse/core";
import { saveAs } from "file-saver";
import { ElMessage, ElMessageBox } from "element-plus";
import { $t } from "@/locales";
import {
  type SupplierConfigForm,
  createEmptyAuthFlow,
  resolveUpstreamType,
  serializeAuthFlow,
} from "@/views/mazbri/suppliers/supplier-form";
import {
  type AssertionSpec,
  type CaptureSpec,
  type ConnectorDraft,
  type ConnectorEndpoint,
  type ConnectorRun,
  type PublishResult,
  createDraft,
  draftFromSupplier,
  exportPostman,
  getDraft,
  importCurl,
  importOpenApi,
  importPostman,
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
import { useUserStore } from "@/store/modules/user";
import { canWriteUpstreamConnector } from "@/utils/ops/roles";
import { copyWithFeedback } from "@/utils/ui/copy-feedback";
import { isHttpError } from "@/utils/http/error";
import { isSessionHttpAuthActive } from "./authMatrix";
import { invalidationFromApi, invalidationSource } from "./authInvalidation";
import {
  parseCryptoProfile,
  validateCryptoProfileClient,
} from "./cryptoProfileHelpers";
import {
  AUTH_PROFILES,
  CODE_FREE_UPSTREAM_TYPE,
  WRITE_METHODS,
  adapterBodyFilled,
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
  findStaticCapturePlaceholders,
  findUnresolvedVars,
  gateViewFromServer,
  genSessionCode,
  importWorkbenchEnvJson,
  isLegalHttpMethod,
  isMicroserviceUpstream,
  isPersistedDraftId,
  kvEnabledCount,
  kvToMap,
  lastAuthJson,
  leftoverTokenHeaderNames,
  mapToKv,
  mergeEnvWithCaptures,
  mergeImportedEndpoints,
  mergeQueryFromMap,
  normalizeCaptureList,
  normalizeWorkbenchEndpoint,
  normalizeWorkbenchEnvDoc,
  parseCurl,
  parseOpenApiDocument,
  parsePostmanCollection,
  recordFromUnknown,
  resolveActiveEnvMap,
  resolvedBodyEncoding,
  sanitizeAuthSpec,
  serializeCaptureList,
  serializeWorkbenchEnvDoc,
  setBodyEncoding,
  tlsFormToSpec,
  tlsSpecToForm,
  trySplitRequestUrl,
  validateTlsFormClient,
  workbenchEnvSurfaceParts,
  type ChannelDraft,
  type KvRow,
  type ParsedImport,
  type SplitDirection,
  type TlsFormState,
  type WorkbenchEnvState,
} from "./workbenchHelpers";

export type SidebarPanelId = "endpoints" | "env" | "gate" | "history";
export type RequestSubTabId =
  | "params"
  | "body"
  | "headers"
  | "auth"
  | "transform"
  | "assert"
  | "captures"
  | "tls";
export type ResponseSubTabId =
  | "json"
  | "raw"
  | "headers"
  | "assertions"
  | "pipeline";

export interface EndpointTabRuntime {
  reqTab: RequestSubTabId;
  resTab: ResponseSubTabId;
  running: boolean;
  lastRun: ConnectorRun | null;
  abort: AbortController | null;
  runGen: number;
}

const WORKBENCH_PATH = "/operations/upstream-supply/workbench";
const LAYOUT_STORAGE_KEY = "mazbri:wb:ide-layout";
const RUN_HTTP = { showErrorMessage: false as const, timeout: 120_000 };

function createDefaultTabRuntime(): EndpointTabRuntime {
  return {
    reqTab: "params",
    resTab: "json",
    running: false,
    lastRun: null,
    abort: null,
    runGen: 0,
  };
}

function readPersistedIdeLayout(): {
  sidebarPanel: SidebarPanelId;
  sidebarCollapsed: boolean;
  responseCollapsed: boolean;
  responseSplitDir: SplitDirection;
  sidebarSize: number;
  requestSize: number;
} {
  const defaults = {
    sidebarPanel: "endpoints" as SidebarPanelId,
    sidebarCollapsed: false,
    responseCollapsed: false,
    responseSplitDir: "horizontal" as SplitDirection,
    sidebarSize: 22,
    requestSize: 52,
  };
  if (typeof localStorage === "undefined") return defaults;
  try {
    const raw = localStorage.getItem(LAYOUT_STORAGE_KEY);
    if (!raw) return defaults;
    const p = JSON.parse(raw) as Record<string, unknown>;
    const panel = p.sidebarPanel;
    return {
      sidebarPanel:
        panel === "endpoints" ||
        panel === "env" ||
        panel === "gate" ||
        panel === "history"
          ? panel
          : "endpoints",
      sidebarCollapsed: Boolean(p.sidebarCollapsed),
      responseCollapsed: Boolean(p.responseCollapsed),
      responseSplitDir:
        p.responseSplitDir === "vertical" ? "vertical" : "horizontal",
      sidebarSize:
        typeof p.sidebarSize === "number" &&
        p.sidebarSize >= 14 &&
        p.sidebarSize <= 42
          ? p.sidebarSize
          : 22,
      requestSize:
        typeof p.requestSize === "number" &&
        p.requestSize >= 24 &&
        p.requestSize <= 78
          ? p.requestSize
          : 52,
    };
  } catch {
    return defaults;
  }
}

export function useWorkbenchStore() {
  const route = useRoute();
  const router = useRouter();
  const userStore = useUserStore();

  const isSuperAdmin = computed(() => userStore.isAdmin);
  const canWriteWorkbench = computed(() =>
    canWriteUpstreamConnector(userStore.info?.role),
  );
  const canForcePublish = computed(() => canWriteWorkbench.value);

  const persistedLayout = readPersistedIdeLayout();
  const activeSidebarPanel = ref<SidebarPanelId>(persistedLayout.sidebarPanel);
  const sidebarCollapsed = ref<boolean>(persistedLayout.sidebarCollapsed);
  const responseCollapsed = ref<boolean>(persistedLayout.responseCollapsed);
  const responseSplitDir = ref<SplitDirection>(persistedLayout.responseSplitDir);
  const sidebarSize = ref<number>(persistedLayout.sidebarSize);
  const requestSize = ref<number>(persistedLayout.requestSize);

  watch(
    [
      activeSidebarPanel,
      sidebarCollapsed,
      responseCollapsed,
      responseSplitDir,
      sidebarSize,
      requestSize,
    ],
    () => {
      if (typeof localStorage === "undefined") return;
      try {
        localStorage.setItem(
          LAYOUT_STORAGE_KEY,
          JSON.stringify({
            sidebarPanel: activeSidebarPanel.value,
            sidebarCollapsed: sidebarCollapsed.value,
            responseCollapsed: responseCollapsed.value,
            responseSplitDir: responseSplitDir.value,
            sidebarSize: sidebarSize.value,
            requestSize: requestSize.value,
          }),
        );
      } catch {
        void 0;
      }
    },
  );

  const draft = ref<ConnectorDraft | null>(null);
  const recentDrafts = ref<ConnectorDraft[]>([]);
  const transparentSuppliers = ref<Supplier[]>([]);
  const switcherLoading = ref(false);

  const endpoints = ref<ConnectorEndpoint[]>([]);
  const openTabIds = ref<string[]>([]);
  const activeTabId = ref<string>("");
  const tabRuntimeMap = reactive<Record<string, EndpointTabRuntime>>({});

  const runs = ref<ConnectorRun[]>([]);
  const envState = ref<WorkbenchEnvState>({
    activeEnvId: null,
    globalRows: [],
    environments: [],
  });
  const capturedVars = ref<Record<string, string>>({});

  const headerRows = ref<KvRow[]>([]);
  const queryRows = ref<KvRow[]>([]);
  const gqlVarRows = ref<KvRow[]>([]);
  const gqlEnumRows = ref<KvRow[]>([]);
  const urlDraft = ref("");
  const urlFocused = ref(false);
  const urlMissing = ref(false);
  const uploadingArtifact = ref(false);

  const configForm = ref<SupplierConfigForm>({
    upstream_type: CODE_FREE_UPSTREAM_TYPE,
    auth_flow: createEmptyAuthFlow(),
  });
  const supplierTimeoutMs = ref<number>(15000);
  const supplierTlsForm = ref<TlsFormState>(emptyTlsForm());
  const endpointTlsForm = ref<TlsFormState>(emptyTlsForm());

  const bootstrapping = ref(true);
  const saving = ref(false);
  const dirty = ref(false);
  const suppressDirty = ref(false);
  const editGen = ref(0);
  const saveError = ref(false);
  const lastSavedAt = ref<Date | null>(null);
  const leavePromptOpen = ref(false);

  const importing = ref(false);
  const publishing = ref(false);
  const showImport = ref(false);
  const showPublish = ref(false);
  const showCommands = ref(false);
  const showShortcuts = ref(false);
  const importTab = ref("curl");
  const importMode = ref("merge");
  const importCurlText = ref("");
  const importJsonText = ref("");

  const publishForm = ref({
    mode: "supplier_channels" as
      | "supplier_only"
      | "supplier_channels"
      | "full_onboarding",
    supplier_code: "",
    supplier_name: "",
    product_id: "",
    require_gate: true,
    force_publish: false,
    force_reason: "",
    comment: "",
  });
  const publishChannels = ref<ChannelDraft[]>([]);
  const publishResult = ref<PublishResult | null>(null);

  function ensureTabRuntime(epId: string): EndpointTabRuntime {
    if (!epId) return createDefaultTabRuntime();
    if (!tabRuntimeMap[epId]) {
      tabRuntimeMap[epId] = createDefaultTabRuntime();
    }
    return tabRuntimeMap[epId]!;
  }

  const activeEpIdx = computed(() => {
    if (!endpoints.value.length) return -1;
    const idx = endpoints.value.findIndex((ep) => ep.id === activeTabId.value);
    return idx >= 0 ? idx : 0;
  });

  const activeEp = computed<ConnectorEndpoint | null>(
    () => endpoints.value[activeEpIdx.value] || null,
  );

  const openTabs = computed(() =>
    openTabIds.value
      .map((id) => {
        const index = endpoints.value.findIndex((ep) => ep.id === id);
        if (index < 0) return null;
        return {
          id,
          index,
          ep: endpoints.value[index]!,
          runtime: ensureTabRuntime(id),
        };
      })
      .filter((x): x is NonNullable<typeof x> => x !== null),
  );

  const activeRuntime = computed<EndpointTabRuntime>(() => {
    const id = activeEp.value?.id || "";
    return ensureTabRuntime(id);
  });

  const reqTab = computed<RequestSubTabId>({
    get: () => activeRuntime.value.reqTab,
    set: (v) => {
      activeRuntime.value.reqTab = v;
    },
  });

  const resTab = computed<ResponseSubTabId>({
    get: () => activeRuntime.value.resTab,
    set: (v) => {
      activeRuntime.value.resTab = v;
    },
  });

  const running = computed(() => activeRuntime.value.running);
  const lastRun = computed<ConnectorRun | null>({
    get: () => activeRuntime.value.lastRun,
    set: (v) => {
      activeRuntime.value.lastRun = v;
    },
  });

  const staticEnvMap = computed(() => resolveActiveEnvMap(envState.value));
  const runtimeEnvMap = computed(() =>
    mergeEnvWithCaptures(staticEnvMap.value, capturedVars.value),
  );

  const activeNamedEnv = computed(
    () =>
      envState.value.environments.find(
        (e) => e.id === envState.value.activeEnvId,
      ) || null,
  );

  const capturedCount = computed(
    () => Object.keys(capturedVars.value).length,
  );
  const envVarCount = computed(
    () => Object.keys(staticEnvMap.value).length,
  );

  const isEditMode = computed(
    () =>
      typeof route.query.supplier_id === "string" &&
      route.query.supplier_id.length > 0,
  );

  const isSupplierLinked = computed(() =>
    Boolean(draft.value?.linked_supplier_id || isEditMode.value),
  );

  const saveBadge = computed(() => {
    if (saving.value) {
      return {
        type: "info" as const,
        text: $t("upstreamConnector.savingDraft"),
      };
    }
    if (saveError.value) {
      return {
        type: "danger" as const,
        text: $t("upstreamConnector.saveDraftFailed"),
      };
    }
    if (dirty.value) {
      return {
        type: "warning" as const,
        text: $t("upstreamConnector.unsaved"),
      };
    }
    if (lastSavedAt.value && isPersistedDraftId(draft.value?.id)) {
      return {
        type: "success" as const,
        text: $t("upstreamConnector.saved"),
      };
    }
    return null;
  });

  function isSuccessfulSend(r: ConnectorRun | null | undefined): boolean {
    return Boolean(r?.ok) && r?.mode !== "transform_preview";
  }

  const canPublish = computed(
    () =>
      runs.value.some((r) => isSuccessfulSend(r)) ||
      isSuccessfulSend(lastRun.value),
  );

  const gate = computed(() =>
    gateViewFromServer(endpoints.value, draft.value?.gate),
  );

  const leftoverTokens = computed(() =>
    leftoverTokenHeaderNames({
      endpoints: endpoints.value,
      authEnabled: isSessionHttpAuthActive(
        configForm.value.auth_flow.enabled,
        configForm.value.auth_flow.profile,
      ),
      injectTo: configForm.value.auth_flow.inject_to,
      injectKey: configForm.value.auth_flow.inject_key,
    }),
  );

  const activeInvalidationSource = computed(() =>
    invalidationSource(
      (activeEp.value?.auth as Record<string, unknown> | null | undefined) ??
        null,
      configForm.value.auth_flow,
    ),
  );

  const activeAssertions = computed<AssertionSpec[]>({
    get: () => activeEp.value?.assertions || [],
    set: (v) => {
      if (activeEp.value) activeEp.value.assertions = v;
    },
  });

  const activeCaptures = computed<CaptureSpec[]>({
    get: () => activeEp.value?.captures || [],
    set: (v) => {
      if (activeEp.value) activeEp.value.captures = v;
    },
  });

  const sessionCredentialKeys = computed(() =>
    configForm.value.auth_flow.credentials
      .map((item) => item.key.trim())
      .filter(Boolean),
  );

  const authSeedJson = computed(() => lastAuthJson(lastRun.value));
  const queryCount = computed(() => kvEnabledCount(queryRows.value));
  const headerCount = computed(() => kvEnabledCount(headerRows.value));
  const captureCount = computed(
    () => serializeCaptureList(activeEp.value?.captures).length,
  );
  const bodyFilled = computed(() =>
    activeEp.value ? adapterBodyFilled(activeEp.value) : false,
  );
  const authFilled = computed(() => {
    if (
      isSessionHttpAuthActive(
        configForm.value.auth_flow.enabled,
        configForm.value.auth_flow.profile,
      )
    ) {
      return true;
    }
    const profile = String(activeEp.value?.auth?.profile || "none");
    return (
      (AUTH_PROFILES as readonly string[]).includes(profile) &&
      profile !== "none"
    );
  });
  const transformFilled = computed(() => {
    const ep = activeEp.value;
    if (!ep) return false;
    if ((ep.adapter || "http_json") !== "http_json") return true;
    if ((ep.operation || "").trim()) return true;
    if ((ep.unwrap || "").trim()) return true;
    const wrap = (ep.wrap || "").trim();
    if (Boolean(wrap) && wrap !== "none") return true;
    return Boolean(ep.crypto && typeof ep.crypto === "object");
  });
  const assertFilled = computed(() =>
    (activeEp.value?.assertions || []).some((a) => (a.type || "").trim()),
  );
  const tlsFilled = computed(() => {
    const ep = activeEp.value;
    return Boolean(
      supplierTlsForm.value.enabled ||
        endpointTlsForm.value.enabled ||
        (ep?.timeout_ms && ep.timeout_ms > 0) ||
        (ep?.base_url || "").trim() ||
        (ep?.request_id_header || "").trim(),
    );
  });

  function unresolvedForRun(ep: ConnectorEndpoint | null | undefined): string[] {
    if (!ep) return [];
    return findUnresolvedVars(
      workbenchEnvSurfaceParts({
        upstreamUrl: draft.value?.upstream_url,
        endpoint: ep,
        authFlow: configForm.value.auth_flow,
      }),
      runtimeEnvMap.value,
    );
  }

  function unresolvedForPublish(
    ep: ConnectorEndpoint | null | undefined,
  ): string[] {
    if (!ep) return [];
    const staticCaptures = findStaticCapturePlaceholders({
      upstreamUrl: draft.value?.upstream_url,
      endpoint: ep,
      authFlow: configForm.value.auth_flow,
    });
    const staticUnresolved = findUnresolvedVars(
      workbenchEnvSurfaceParts({
        upstreamUrl: draft.value?.upstream_url,
        endpoint: { ...ep, body: undefined },
        authFlow: configForm.value.auth_flow,
      }),
      staticEnvMap.value,
    );
    return [...new Set([...staticCaptures, ...staticUnresolved])];
  }

  const unresolved = computed(() => unresolvedForRun(activeEp.value));
  const activeStaticCaptureWarnings = computed(() =>
    findStaticCapturePlaceholders({
      upstreamUrl: draft.value?.upstream_url,
      endpoint: activeEp.value,
      authFlow: configForm.value.auth_flow,
    }),
  );

  const unresolvedAll = computed(() => {
    const names = new Set<string>();
    for (const ep of endpoints.value) {
      for (const n of unresolvedForPublish(ep)) names.add(n);
    }
    if (!endpoints.value.length) {
      for (const n of unresolvedForPublish({ method: "GET", path: "/" })) {
        names.add(n);
      }
    }
    return [...names];
  });

  const publishResultDesc = computed(() => {
    if (!publishResult.value) return "";
    const r = publishResult.value;
    return `supplier=${r.supplier_id} version=v${r.version_no} route_version=${r.route_version ?? "—"}`;
  });

  function apiConfigToForm(cfg: Record<string, unknown>): SupplierConfigForm {
    const auth = (cfg.auth_flow || {}) as Record<string, unknown>;
    const creds = (auth.credentials || {}) as Record<string, string>;
    const credsList = Object.entries(creds).map(([key, value]) => ({
      key,
      value: String(value ?? ""),
    }));
    const rawProfile = String(auth.profile || "");
    const httpActive = isSessionHttpAuthActive(Boolean(auth.enabled), rawProfile);
    const timeouts = (cfg.timeouts || {}) as Record<string, unknown>;
    supplierTimeoutMs.value =
      typeof timeouts.total_ms === "number" && timeouts.total_ms > 0
        ? timeouts.total_ms
        : 15000;
    supplierTlsForm.value = tlsSpecToForm(
      cfg.tls as Record<string, unknown> | null | undefined,
    );
    return {
      upstream_type: CODE_FREE_UPSTREAM_TYPE,
      auth_flow: {
        ...createEmptyAuthFlow(),
        enabled: httpActive,
        profile: rawProfile,
        token_url: String(auth.token_url || ""),
        token_path: String(auth.token_path || ""),
        method: String(auth.method || "POST"),
        headers: mapToKv(auth.headers as Record<string, string> | undefined),
        request_body_tpl: String(auth.request_body_tpl || ""),
        credentials: credsList.length
          ? credsList
          : createEmptyAuthFlow().credentials,
        token_jsonpath: String(auth.token_jsonpath || "$.token"),
        expiry_jsonpath: String(auth.expiry_jsonpath || "$.expires_in"),
        inject_to: String(auth.inject_to || "header"),
        inject_key: String(auth.inject_key || "Authorization"),
        inject_value_format: String(auth.inject_value_format || "Bearer {}"),
        jwt: { ...createEmptyAuthFlow().jwt, ...((auth.jwt as object) || {}) },
        digest: {
          ...createEmptyAuthFlow().digest,
          ...((auth.digest as object) || {}),
        },
        aws: { ...createEmptyAuthFlow().aws, ...((auth.aws as object) || {}) },
        hawk: {
          ...createEmptyAuthFlow().hawk,
          ...((auth.hawk as object) || {}),
        },
        invalidation: invalidationFromApi(auth.invalidation),
      },
    };
  }

  function formToApiConfig(form: SupplierConfigForm): Record<string, unknown> {
    const credsMap: Record<string, string> = {};
    form.auth_flow.credentials.forEach((item) => {
      if (item.key.trim()) credsMap[item.key.trim()] = item.value;
    });
    const hasCreds = Object.keys(credsMap).length > 0;
    const httpActive = isSessionHttpAuthActive(
      form.auth_flow.enabled,
      form.auth_flow.profile,
    );
    const payload: Record<string, unknown> = {
      upstream_type: CODE_FREE_UPSTREAM_TYPE,
      auth_flow: httpActive
        ? serializeAuthFlow(form.auth_flow, credsMap)
        : hasCreds
          ? { enabled: false, profile: "none", credentials: credsMap }
          : null,
      timeouts: {
        total_ms:
          Number.isFinite(supplierTimeoutMs.value) &&
          supplierTimeoutMs.value > 0
            ? Math.round(supplierTimeoutMs.value)
            : 15000,
      },
    };
    const tls = tlsFormToSpec(supplierTlsForm.value);
    if (tls) payload.tls = tls;
    return payload;
  }

  function syncKvFromActive() {
    const ep = activeEp.value;
    headerRows.value = entriesToKv(ep?.header_entries, ep?.headers || {});
    queryRows.value = entriesToKv(ep?.query_entries, ep?.query || {});
    gqlVarRows.value = mapToKv(recordFromUnknown(ep?.graphql?.variables_map));
    gqlEnumRows.value = mapToKv(recordFromUnknown(ep?.graphql?.enum_map));
    endpointTlsForm.value = tlsSpecToForm(ep?.tls ?? null);
  }

  function flushKvToEndpoint(ep: ConnectorEndpoint | null | undefined) {
    if (!ep) return;
    ep.header_entries = headerRows.value.map((r) => ({
      key: r.key,
      value: r.value,
      desc: r.desc ?? "",
      enabled: r.enabled !== false,
    }));
    ep.query_entries = queryRows.value.map((r) => ({
      key: r.key,
      value: r.value,
      desc: r.desc ?? "",
      enabled: r.enabled !== false,
    }));
    ep.headers = kvToMap(headerRows.value);
    ep.query = kvToMap(queryRows.value);
    ep.tls = tlsFormToSpec(endpointTlsForm.value);
    ep.captures = normalizeCaptureList(ep.captures);
    if (ep.adapter === "graphql") {
      ensureEndpointAdapterFields(ep);
      ep.graphql = {
        ...(ep.graphql || {}),
        variables_map: kvToMap(gqlVarRows.value),
        enum_map: kvToMap(gqlEnumRows.value),
      };
    }
  }

  function flushKvToActive() {
    flushKvToEndpoint(activeEp.value);
  }

  function composedUrl() {
    const base =
      (activeEp.value?.base_url || "").trim() ||
      draft.value?.upstream_url ||
      "";
    return composeRequestUrl(base, activeEp.value?.path || "", queryRows.value);
  }

  function refreshUrlDraft() {
    if (urlFocused.value) return;
    urlDraft.value = composedUrl();
  }

  function applyRequestUrl(raw: string) {
    if (!draft.value || !activeEp.value) return;
    const parts = trySplitRequestUrl(raw);
    if (!parts) return;
    if (parts.base) {
      if ((activeEp.value.base_url || "").trim()) {
        activeEp.value.base_url = parts.base;
      } else {
        draft.value.upstream_url = parts.base;
      }
    }
    activeEp.value.path = parts.path;
    queryRows.value = mergeQueryFromMap(parts.query, queryRows.value);
  }

  function onUrlInput(raw: string) {
    urlDraft.value = raw;
    applyRequestUrl(raw);
  }

  function onUrlBlur() {
    urlFocused.value = false;
    applyRequestUrl(urlDraft.value);
    urlDraft.value = composedUrl();
  }

  function markDirty() {
    if (!suppressDirty.value) {
      dirty.value = true;
      editGen.value += 1;
    }
  }

  async function refreshSwitcherOptions() {
    switcherLoading.value = true;
    try {
      const [draftRes, supRes] = await Promise.allSettled([
        listDrafts({ page: 1, page_size: 30 }),
        fetchSuppliers({ page: 1, page_size: 50 }),
      ]);
      if (draftRes.status === "fulfilled") {
        recentDrafts.value = draftRes.value.drafts || [];
      }
      if (supRes.status === "fulfilled") {
        transparentSuppliers.value = (supRes.value.suppliers || []).filter(
          (s) => resolveUpstreamType(s) !== "microservice",
        );
      }
    } finally {
      switcherLoading.value = false;
    }
  }

  async function syncSessionRoute(d: ConnectorDraft) {
    if (d.linked_supplier_id) {
      if (
        route.query.supplier_id === d.linked_supplier_id &&
        !route.query.session
      ) {
        return;
      }
      await router.replace({
        path: WORKBENCH_PATH,
        query: { supplier_id: d.linked_supplier_id },
      });
      return;
    }
    if (!isPersistedDraftId(d.id)) {
      if (!route.query.session && !route.query.supplier_id) return;
      await router.replace({ path: WORKBENCH_PATH });
      return;
    }
    if (route.query.session === d.id && !route.query.supplier_id) return;
    await router.replace({
      path: WORKBENCH_PATH,
      query: { session: d.id },
    });
  }

  function leaveForMicroservice(supplierId?: string | null) {
    ElMessage.warning($t("upstreamConnector.transparentOnly"));
    if (supplierId) {
      void router.replace({
        name: "SupplierDetail",
        params: { id: supplierId },
      });
      return;
    }
    void router.replace({
      path: "/operations/upstream-supply",
      query: { tab: "suppliers" },
    });
  }

  function assignRunsToEndpointTabs(runList: ConnectorRun[]) {
    for (const ep of endpoints.value) {
      if (!ep.id) continue;
      const rt = ensureTabRuntime(ep.id);
      const hit = runList.find(
        (r) =>
          r.endpoint_key &&
          (r.endpoint_key === ep.id ||
            r.endpoint_key === ep.name ||
            r.endpoint_key === ep.path),
      );
      rt.lastRun = hit || null;
    }
    if (
      activeEp.value?.id &&
      !ensureTabRuntime(activeEp.value.id).lastRun &&
      runList[0]
    ) {
      ensureTabRuntime(activeEp.value.id).lastRun = runList[0];
    }
  }

  async function openSession(
    d: ConnectorDraft,
    prefetchedSupplier?: Supplier,
  ): Promise<boolean> {
    if (isMicroserviceUpstream(d) && d.linked_supplier_id) {
      draft.value = null;
      leaveForMicroservice(d.linked_supplier_id);
      return false;
    }
    const coerced = isMicroserviceUpstream(d);
    if (coerced) {
      d = asCodeFreeDraft(d);
    }
    abortAllRuns();
    suppressDirty.value = true;
    Object.keys(tabRuntimeMap).forEach((k) => {
      delete tabRuntimeMap[k];
    });
    capturedVars.value = {};
    draft.value = {
      ...d,
      upstream_type: CODE_FREE_UPSTREAM_TYPE,
      endpoints: Array.isArray(d.endpoints) ? d.endpoints : [],
      config: { ...(d.config || {}), upstream_type: CODE_FREE_UPSTREAM_TYPE },
    };
    configForm.value = apiConfigToForm(
      (draft.value.config || {}) as Record<string, unknown>,
    );
    endpoints.value = (draft.value.endpoints || []).map((ep, i) =>
      normalizeWorkbenchEndpoint(ep, i),
    );
    const firstId = endpoints.value[0]?.id || "";
    openTabIds.value = firstId ? [firstId] : [];
    activeTabId.value = firstId;

    if (d.linked_supplier_id) {
      try {
        const sup =
          prefetchedSupplier?.id === d.linked_supplier_id
            ? prefetchedSupplier
            : await fetchSupplierById(d.linked_supplier_id);
        publishForm.value.supplier_code = sup.supplier_code;
        publishForm.value.supplier_name = sup.name;
        if (draft.value) draft.value.name = sup.name;
      } catch {
        publishForm.value.supplier_code = d.draft_code
          .replace(/^from-/i, "")
          .replace(/-[a-f0-9]{6}$/i, "")
          .toUpperCase();
        publishForm.value.supplier_name = d.name;
      }
    } else {
      publishForm.value.supplier_code =
        d.name
          .replace(/[^A-Za-z0-9_-]/g, "-")
          .replace(/-+/g, "-")
          .replace(/^-|-$/g, "")
          .toUpperCase()
          .slice(0, 32) || genSessionCode().toUpperCase();
      publishForm.value.supplier_name = d.name;
    }
    publishResult.value = null;
    envState.value = normalizeWorkbenchEnvDoc(d.workbench_env ?? []);
    syncKvFromActive();
    refreshUrlDraft();
    saveError.value = false;
    lastSavedAt.value = isPersistedDraftId(d.id)
      ? new Date(d.updated_at)
      : null;

    if (isPersistedDraftId(d.id)) {
      try {
        runs.value = (await listRuns(d.id)) || [];
      } catch {
        runs.value = [];
      }
    } else {
      runs.value = [];
    }
    assignRunsToEndpointTabs(runs.value);
    for (const r of [...runs.value].reverse()) {
      const cap = r.response_preview?.captured;
      if (cap && typeof cap === "object") {
        Object.assign(capturedVars.value, cap);
      }
    }

    await syncSessionRoute(d);
    void refreshSwitcherOptions();
    await nextTick();
    dirty.value = coerced;
    if (coerced) editGen.value += 1;
    suppressDirty.value = false;
    if (!coerced) {
      await nextTick();
      dirty.value = false;
    }
    return true;
  }

  function switchEndpointByIndex(nextIdx: number) {
    const ep = endpoints.value[nextIdx];
    if (!ep) return;
    if (!ep.id) ep.id = `ep-${nextIdx + 1}`;
    openEndpointTab(ep.id);
  }

  function openEndpointTab(epId: string) {
    if (!epId || epId === activeTabId.value) return;
    const prevEp = activeEp.value;
    suppressDirty.value = true;
    flushKvToEndpoint(prevEp);
    if (!openTabIds.value.includes(epId)) {
      openTabIds.value = [...openTabIds.value, epId];
    }
    ensureTabRuntime(epId);
    activeTabId.value = epId;
    syncKvFromActive();
    refreshUrlDraft();
    void nextTick(() => {
      suppressDirty.value = false;
    });
  }

  function closeEndpointTab(epId: string) {
    const pos = openTabIds.value.indexOf(epId);
    if (pos < 0) return;
    if (openTabIds.value.length <= 1) return;
    const nextTabs = openTabIds.value.filter((id) => id !== epId);
    if (activeTabId.value === epId) {
      const fallback = nextTabs[Math.max(0, pos - 1)] || nextTabs[0] || "";
      openTabIds.value = nextTabs;
      if (fallback) openEndpointTab(fallback);
    } else {
      openTabIds.value = nextTabs;
    }
  }

  function addEndpoint(defaultTag = "") {
    if (!canWriteWorkbench.value) return;
    flushKvToActive();
    const created = normalizeWorkbenchEndpoint(
      {
        id: `ep-${Date.now().toString(36)}`,
        name: `endpoint-${endpoints.value.length + 1}`,
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
      endpoints.value.length,
    );
    endpoints.value.push(created);
    if (created.id) openEndpointTab(created.id);
  }

  function onEpAction({ cmd, index }: { cmd: string; index: number }) {
    if (!canWriteWorkbench.value) return;
    flushKvToActive();
    if (cmd === "duplicate") {
      const src = endpoints.value[index];
      if (!src) return;
      const cloned: ConnectorEndpoint = normalizeWorkbenchEndpoint(
        {
          ...(JSON.parse(JSON.stringify(src)) as ConnectorEndpoint),
          id: `ep-${Date.now().toString(36)}`,
          name: `${src.name || "endpoint"}-copy`,
        },
        index + 1,
      );
      endpoints.value.splice(index + 1, 0, cloned);
      if (cloned.id) openEndpointTab(cloned.id);
      return;
    }
    if (cmd === "delete") {
      if (endpoints.value.length <= 1) {
        ElMessage.warning($t("upstreamConnector.keepOneEndpoint"));
        return;
      }
      const victim = endpoints.value[index];
      const victimId = victim?.id || "";
      endpoints.value.splice(index, 1);
      if (victimId) {
        delete tabRuntimeMap[victimId];
        openTabIds.value = openTabIds.value.filter((id) => id !== victimId);
      }
      if (!openTabIds.value.length && endpoints.value[0]?.id) {
        openTabIds.value = [endpoints.value[0].id];
      }
      if (
        !activeTabId.value ||
        !endpoints.value.some((e) => e.id === activeTabId.value)
      ) {
        const nextId = openTabIds.value[0] || endpoints.value[0]?.id || "";
        suppressDirty.value = true;
        activeTabId.value = nextId;
        syncKvFromActive();
        refreshUrlDraft();
        void nextTick(() => {
          suppressDirty.value = false;
        });
      }
    }
  }

  function onEpReorder(from: number, to: number) {
    if (from === to || from < 0 || to < 0) return;
    const list = endpoints.value;
    if (from >= list.length || to >= list.length) return;
    const next = [...list];
    const [moved] = next.splice(from, 1);
    if (!moved) return;
    next.splice(to, 0, moved);
    endpoints.value = next;
  }

  function toggleGate(index: number) {
    const ep = endpoints.value[index];
    if (!ep) return;
    ep.gate = ep.gate === false;
  }

  function setEpAuthProfile(profile: string) {
    const ep = activeEp.value;
    if (!ep) return;
    const next = profile === "inherit" ? "none" : profile;
    ep.auth = { ...(ep.auth || {}), profile: next };
  }

  function onCryptoProfileChange(value: Record<string, unknown> | null) {
    if (!activeEp.value) return;
    activeEp.value.crypto = value;
  }

  function onBodyEncodingChange(value: string) {
    const ep = activeEp.value;
    if (!ep) return;
    const enc =
      value === "form" || value === "none" || value === "json" ? value : "json";
    setBodyEncoding(ep, enc);
  }

  function onAdapterQuickSelect(next: string) {
    if (!canWriteWorkbench.value || !activeEp.value) return;
    activeEp.value.adapter = next;
    ensureEndpointAdapterFields(activeEp.value);
    syncKvFromActive();
  }

  function addNamedEnv(name: string) {
    if (!canWriteWorkbench.value) return;
    const created = createNamedEnv(name);
    envState.value.environments.push(created);
    envState.value.activeEnvId = created.id;
  }

  function deleteNamedEnv(id: string) {
    if (!canWriteWorkbench.value) return;
    envState.value.environments = envState.value.environments.filter(
      (e) => e.id !== id,
    );
    if (envState.value.activeEnvId === id) {
      envState.value.activeEnvId =
        envState.value.environments[0]?.id ?? null;
    }
  }

  function setActiveEnv(id: string | null) {
    envState.value.activeEnvId = id || null;
  }

  function exportEnvToFile() {
    const doc = exportWorkbenchEnvJson(envState.value, draft.value?.name);
    const blob = new Blob([JSON.stringify(doc, null, 2)], {
      type: "application/json",
    });
    saveAs(blob, `${draft.value?.name || "workbench"}.env.json`);
  }

  function importEnvFromText(rawText: string) {
    if (!canWriteWorkbench.value) return;
    envState.value = importWorkbenchEnvJson(rawText, envState.value);
    ElMessage.success($t("upstreamConnector.envImportOk"));
  }

  function clearCapturedVars() {
    capturedVars.value = {};
  }

  function removeCapturedVar(name: string) {
    const next = { ...capturedVars.value };
    delete next[name];
    capturedVars.value = next;
  }

  function cryptoIssuesOnEndpoints(): string[] {
    const msgs: string[] = [];
    for (const ep of endpoints.value) {
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
    const msgs: string[] = [];
    flushKvToActive();
    for (const code of validateTlsFormClient(supplierTlsForm.value)) {
      msgs.push(`${$t("upstreamConnector.tlsSupplierSection")}: ${$t(`upstreamConnector.tlsErr.${code}`)}`);
    }
    for (const ep of endpoints.value) {
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

  function sessionHasPersistableContent(): boolean {
    const d = draft.value;
    if (!d) return false;
    const untitled = $t("upstreamConnector.untitledName");
    if (d.name.trim() && d.name.trim() !== untitled) return true;
    if (d.upstream_url.trim()) return true;
    if (envState.value.globalRows.some((row) => row.key.trim())) return true;
    if (envState.value.environments.length > 0) return true;
    if (endpoints.value.length > 1) return true;
    const ep = endpoints.value[0];
    if (!ep) return false;
    if ((ep.path || "").trim()) return true;
    if ((ep.name || "").trim() && !/^endpoint-\d+$/i.test(ep.name || "")) {
      return true;
    }
    if ((ep.method || "GET").toUpperCase() !== "GET") return true;
    return false;
  }

  function shouldGuardLeave(): boolean {
    if (!canWriteWorkbench.value) return false;
    if (!draft.value || bootstrapping.value) return false;
    if (!dirty.value) return false;
    return isPersistedDraftId(draft.value.id) || sessionHasPersistableContent();
  }

  async function saveDraft(opts?: {
    silent?: boolean;
    signal?: AbortSignal;
    skipRouteSync?: boolean;
  }) {
    if (!canWriteWorkbench.value || !draft.value) return;
    const tlsMsgs = tlsIssuesAll();
    if (tlsMsgs.length) {
      if (!opts?.silent) ElMessage.warning(tlsMsgs[0]!);
      saveError.value = true;
      throw new Error(tlsMsgs[0]);
    }
    saving.value = true;
    suppressDirty.value = true;
    try {
      flushKvToActive();
      const serializedEndpoints = endpoints.value.map((ep) => {
        const nextEp: ConnectorEndpoint = {
          ...ep,
          captures: serializeCaptureList(ep.captures),
        };
        if (nextEp.auth) {
          const nextAuth = sanitizeAuthSpec(
            nextEp.auth as Record<string, unknown>,
          );
          nextEp.auth = nextAuth || { profile: "none" };
        }
        return nextEp;
      });
      const body = {
        name: draft.value.name,
        upstream_url: draft.value.upstream_url,
        upstream_type: CODE_FREE_UPSTREAM_TYPE,
        config: formToApiConfig(configForm.value),
        endpoints: serializedEndpoints,
        workbench_env: serializeWorkbenchEnvDoc(envState.value),
      };
      const reqOpts = {
        ...(opts?.signal ? { signal: opts.signal, ...RUN_HTTP } : {}),
        showErrorMessage: !opts?.silent,
      };
      const updated = isPersistedDraftId(draft.value.id)
        ? await updateDraft(draft.value.id, body, reqOpts)
        : await createDraft(
            {
              draft_code: draft.value.draft_code || genSessionCode(),
              ...body,
              source: "manual",
            },
            reqOpts,
          );
      draft.value = {
        ...draft.value,
        ...updated,
        endpoints: endpoints.value,
        upstream_type: CODE_FREE_UPSTREAM_TYPE,
      };
      supplierTlsForm.value = tlsSpecToForm(
        (updated.config?.tls as Record<string, unknown> | undefined) ?? null,
      );
      if (Array.isArray(updated.endpoints)) {
        updated.endpoints.forEach((srvEp, i) => {
          if (endpoints.value[i] && srvEp.tls) {
            endpoints.value[i]!.tls = srvEp.tls;
          }
        });
        endpointTlsForm.value = tlsSpecToForm(activeEp.value?.tls ?? null);
      }
      dirty.value = false;
      saveError.value = false;
      lastSavedAt.value = new Date();
      await nextTick();
      if (!opts?.skipRouteSync) await syncSessionRoute(draft.value);
      if (!opts?.silent) ElMessage.success($t("common.feedback.saveSuccess"));
    } catch (err) {
      saveError.value = true;
      throw err;
    } finally {
      suppressDirty.value = false;
      saving.value = false;
    }
  }

  watchDebounced(
    editGen,
    async () => {
      if (!canWriteWorkbench.value) return;
      if (
        !dirty.value ||
        bootstrapping.value ||
        saving.value ||
        leavePromptOpen.value ||
        !draft.value
      ) {
        return;
      }
      if (
        !isPersistedDraftId(draft.value.id) &&
        !sessionHasPersistableContent()
      ) {
        return;
      }
      try {
        await saveDraft({ silent: true });
      } catch {
        saveError.value = true;
      }
    },
    { debounce: 900 },
  );

  watch(
    () => activeEp.value?.adapter,
    () => {
      if (!activeEp.value) return;
      ensureEndpointAdapterFields(activeEp.value);
      syncKvFromActive();
    },
  );

  watch(
    [
      () => draft.value?.upstream_url,
      () => activeEp.value?.base_url,
      () => activeEp.value?.path,
      queryRows,
    ],
    () => refreshUrlDraft(),
    { deep: true },
  );

  watch(urlDraft, (v) => {
    if (v.trim()) urlMissing.value = false;
  });

  watch(
    [
      endpoints,
      () => draft.value?.name,
      () => draft.value?.upstream_url,
      configForm,
      supplierTimeoutMs,
      supplierTlsForm,
      endpointTlsForm,
      headerRows,
      queryRows,
      envState,
      gqlVarRows,
      gqlEnumRows,
    ],
    () => markDirty(),
    { deep: true },
  );

  function beginTabRun(epId: string): { signal: AbortSignal; gen: number } {
    responseCollapsed.value = false;
    const rt = ensureTabRuntime(epId);
    rt.abort?.abort();
    const ac = new AbortController();
    rt.abort = ac;
    rt.runGen += 1;
    rt.running = true;
    return { signal: ac.signal, gen: rt.runGen };
  }

  function finishTabRun(epId: string, gen: number) {
    const rt = ensureTabRuntime(epId);
    if (gen !== rt.runGen) return;
    rt.running = false;
    rt.abort = null;
  }

  function abortActiveRun() {
    const id = activeEp.value?.id;
    if (!id) return;
    ensureTabRuntime(id).abort?.abort();
  }

  function abortAllRuns() {
    Object.values(tabRuntimeMap).forEach((rt) => rt.abort?.abort());
  }

  function isRunCancelled(err: unknown): boolean {
    return isHttpError(err) && err.message === $t("httpMsg.requestCancelled");
  }

  async function refreshGate() {
    if (!draft.value || !isPersistedDraftId(draft.value.id)) return;
    try {
      const d = await getDraft(draft.value.id);
      draft.value = { ...draft.value, gate: d.gate };
    } catch {
      void 0;
    }
  }

  function appliedRequest() {
    const ep = activeEp.value;
    if (!ep) return null;
    flushKvToActive();
    const env = runtimeEnvMap.value;
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
        void 0;
      }
    }
    return { method: ep.method || "GET", path, headers, query, body };
  }

  function blockIfUnresolved(names: string[]): boolean {
    if (!names.length) return false;
    ElMessage.warning(
      $t("upstreamConnector.unresolvedVarsBlock", { vars: names.join(", ") }),
    );
    return true;
  }

  function localSendBlock(focusUrl?: () => void): boolean {
    const url = (
      urlDraft.value ||
      activeEp.value?.base_url ||
      draft.value?.upstream_url ||
      ""
    ).trim();
    if (!url) {
      urlMissing.value = true;
      ElMessage.warning($t("upstreamConnector.urlRequired"));
      responseCollapsed.value = false;
      focusUrl?.();
      return true;
    }
    urlMissing.value = false;
    const method = (activeEp.value?.method || "GET").trim();
    if (!isLegalHttpMethod(method)) {
      ElMessage.warning($t("upstreamConnector.methodInvalid"));
      return true;
    }
    return false;
  }

  async function doRun(mode: string, focusUrl?: () => void) {
    const ep = activeEp.value;
    if (!draft.value || !ep || !ep.id) return;
    if (localSendBlock(focusUrl)) return;
    if (blockIfUnresolved(unresolved.value)) return;
    const method = (ep.method || "GET").toUpperCase();
    if (mode === "business" && WRITE_METHODS.has(method)) {
      try {
        await ElMessageBox.confirm(
          $t("upstreamConnector.writeConfirmMsg"),
          $t("upstreamConnector.writeConfirmTitle"),
          { type: "warning" },
        );
      } catch {
        return;
      }
    }
    const epId = ep.id;
    const { signal, gen } = beginTabRun(epId);
    try {
      await saveDraft({ silent: true, signal });
      if (!appliedRequest()) return;
      const r = await runDraft(
        draft.value.id,
        {
          mode,
          endpoint_id: epId,
          assertions: ep.assertions as AssertionSpec[],
          env_id: envState.value.activeEnvId,
          captures: capturedVars.value,
        },
        { signal, ...RUN_HTTP },
      );
      ensureTabRuntime(epId).lastRun = r;
      if (r.response_preview?.captured) {
        capturedVars.value = {
          ...capturedVars.value,
          ...r.response_preview.captured,
        };
      }
      runs.value = [r, ...runs.value].slice(0, 50);
      await refreshGate();
      if (r.ok) ElMessage.success($t("upstreamConnector.runOk"));
      else {
        ElMessage.error(
          r.error_message || r.error_code || $t("upstreamConnector.runFail"),
        );
      }
    } catch (err) {
      if (isRunCancelled(err)) {
        ElMessage.info($t("upstreamConnector.runCancelled"));
        return;
      }
      ElMessage.error(
        isHttpError(err) ? err.message : $t("upstreamConnector.runFail"),
      );
    } finally {
      finishTabRun(epId, gen);
    }
  }

  async function doLiveRun() {
    const ep = activeEp.value;
    if (!draft.value?.linked_supplier_id || !ep || !ep.id) return;
    flushKvToActive();
    const epId = ep.id;
    const channelCode = autoChannelsFromEndpoints(endpoints.value).find(
      (c) => c.endpoint_id === epId,
    )?.channel_code;
    if (!channelCode) return;
    const { signal, gen } = beginTabRun(epId);
    try {
      const r = await liveRun(
        draft.value.linked_supplier_id,
        { channel_code: channelCode, payload: ep.body ?? {} },
        { signal, ...RUN_HTTP },
      );
      ensureTabRuntime(epId).lastRun = r;
      runs.value = [r, ...runs.value].slice(0, 50);
      if (r.ok) ElMessage.success($t("upstreamConnector.runOk"));
      else {
        ElMessage.error(
          r.error_message || r.error_code || $t("upstreamConnector.runFail"),
        );
      }
    } catch (err) {
      if (isRunCancelled(err)) {
        ElMessage.info($t("upstreamConnector.runCancelled"));
        return;
      }
      ElMessage.error(
        isHttpError(err) ? err.message : $t("upstreamConnector.runFail"),
      );
    } finally {
      finishTabRun(epId, gen);
    }
  }

  async function doTransformPreview() {
    const ep = activeEp.value;
    if (!draft.value || !ep || !ep.id) return;
    if (blockIfUnresolved(unresolved.value)) return;
    const epId = ep.id;
    const { signal, gen } = beginTabRun(epId);
    try {
      await saveDraft({ silent: true, signal });
      const r = await transformPreview(
        draft.value.id,
        {
          endpoint_id: epId,
          env_id: envState.value.activeEnvId,
          captures: capturedVars.value,
        },
        { signal, ...RUN_HTTP },
      );
      ensureTabRuntime(epId).lastRun = {
        id: "preview",
        mode: "transform_preview",
        ok: r.ok,
        latency_ms: 0,
        stages: r.stages || [],
        assertions: [],
        created_at: new Date().toISOString(),
      };
      ensureTabRuntime(epId).resTab = "pipeline";
    } catch (err) {
      if (isRunCancelled(err)) {
        ElMessage.info($t("upstreamConnector.runCancelled"));
        return;
      }
      const message = isHttpError(err)
        ? err.message
        : $t("upstreamConnector.runFail");
      ensureTabRuntime(epId).lastRun = {
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
      ElMessage.error(message);
    } finally {
      finishTabRun(epId, gen);
    }
  }

  function applyCurlPaste(parsed: ParsedImport) {
    const src = parsed.endpoints[0];
    if (!src || !draft.value || !activeEp.value) return;
    if (parsed.upstream_url) draft.value.upstream_url = parsed.upstream_url;
    activeEp.value.method = src.method || activeEp.value.method;
    activeEp.value.path = src.path || "/";
    activeEp.value.headers = src.headers || {};
    activeEp.value.query = src.query || {};
    if (src.body !== undefined) activeEp.value.body = src.body;
    headerRows.value = mapToKv(activeEp.value.headers);
    queryRows.value = mapToKv(activeEp.value.query);
    urlDraft.value = composedUrl();
    ElMessage.success($t("upstreamConnector.pasteCurlOk"));
  }

  async function copyCurl() {
    const req = appliedRequest();
    if (!req || !draft.value) return;
    const base =
      (activeEp.value?.base_url || "").trim() || draft.value.upstream_url;
    const text = buildCurl({
      baseUrl: applyEnv(base, runtimeEnvMap.value),
      method: req.method,
      path: req.path,
      headers: req.headers,
      query: req.query,
      body: req.body,
    });
    await copyWithFeedback(text);
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

  async function createBlankSession() {
    if (!canWriteWorkbench.value) return;
    await openSession(buildLocalBlankDraft());
  }

  async function switchToDraft(draftId: string) {
    if (!draftId || draftId === draft.value?.id) return;
    bootstrapping.value = true;
    try {
      const d = await getDraft(draftId);
      await openSession(d);
    } catch (e) {
      ElMessage.error(
        (e as Error)?.message || $t("upstreamConnector.bootstrapFailed"),
      );
    } finally {
      bootstrapping.value = false;
    }
  }

  async function switchToSupplier(supplierId: string) {
    if (!supplierId || supplierId === draft.value?.linked_supplier_id) return;
    bootstrapping.value = true;
    try {
      const sup = await fetchSupplierById(supplierId);
      if (resolveUpstreamType(sup) === "microservice") {
        leaveForMicroservice(supplierId);
        return;
      }
      if (!canWriteWorkbench.value) {
        const d = await previewDraftFromSupplier(supplierId);
        await openSession(d, sup);
        return;
      }
      const d = await draftFromSupplier(supplierId);
      await openSession(d, sup);
    } catch (e) {
      ElMessage.error(
        (e as Error)?.message || $t("upstreamConnector.bootstrapFailed"),
      );
    } finally {
      bootstrapping.value = false;
    }
  }

  async function saveDraftExplicit() {
    if (!canWriteWorkbench.value || !draft.value) return;
    try {
      await saveDraft({ silent: false });
    } catch {
      void 0;
    }
  }

  async function resyncFromLiveSupplier() {
    if (!canWriteWorkbench.value || !draft.value?.linked_supplier_id) return;
    try {
      await ElMessageBox.confirm(
        $t("upstreamConnector.resyncFromLiveConfirm"),
        $t("upstreamConnector.resyncFromLive"),
        { type: "warning" },
      );
    } catch {
      return;
    }
    bootstrapping.value = true;
    try {
      const d = await draftFromSupplier(draft.value.linked_supplier_id, {
        force_resync: true,
      });
      await openSession(d);
      ElMessage.success($t("upstreamConnector.resyncOk"));
    } catch (e) {
      ElMessage.error(
        isHttpError(e) ? e.message : $t("upstreamConnector.bootstrapFailed"),
      );
    } finally {
      bootstrapping.value = false;
    }
  }

  async function onArtifactFile(ev: Event) {
    const input = ev.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file || !activeEp.value) return;
    uploadingArtifact.value = true;
    try {
      const content_base64 = await fileToBase64(file);
      const art = await uploadArtifact({
        artifact_type: "fds",
        name: file.name,
        content_base64,
      });
      activeEp.value.artifact_id = art.id;
      ElMessage.success($t("upstreamConnector.artifactUploaded"));
    } finally {
      uploadingArtifact.value = false;
    }
  }

  async function doImport() {
    if (!canWriteWorkbench.value) return;
    importing.value = true;
    try {
      if (importMode.value === "merge" && draft.value) {
        let parsed: ParsedImport;
        if (importTab.value === "curl") {
          parsed = parseCurl(importCurlText.value);
        } else if (importTab.value === "postman") {
          parsed = parsePostmanCollection(JSON.parse(importJsonText.value));
        } else {
          parsed = parseOpenApiDocument(JSON.parse(importJsonText.value));
        }
        endpoints.value = mergeImportedEndpoints(
          endpoints.value,
          parsed.endpoints,
        ).map((ep, i) => normalizeWorkbenchEndpoint(ep, i));
        if (parsed.upstream_url && !draft.value.upstream_url.trim()) {
          draft.value.upstream_url = parsed.upstream_url;
        }
        if (
          parsed.name &&
          draft.value.name === $t("upstreamConnector.untitledName")
        ) {
          draft.value.name = parsed.name;
        }
        showImport.value = false;
        ElMessage.success(
          $t("upstreamConnector.importedMerge", { n: parsed.endpoints.length }),
        );
        return;
      }
      let d: ConnectorDraft;
      if (importTab.value === "curl") {
        d = await importCurl({ curl: importCurlText.value });
      } else if (importTab.value === "postman") {
        d = await importPostman({ collection: JSON.parse(importJsonText.value) });
      } else {
        d = await importOpenApi({ document: JSON.parse(importJsonText.value) });
      }
      showImport.value = false;
      await openSession(d);
      ElMessage.success($t("upstreamConnector.imported"));
    } catch (e) {
      ElMessage.error((e as Error)?.message || "Import failed");
    } finally {
      importing.value = false;
    }
  }

  async function doExport() {
    if (!canWriteWorkbench.value || !draft.value) return;
    try {
      await saveDraft({ silent: true });
    } catch {
      ElMessage.error($t("upstreamConnector.saveDraftFailed"));
      return;
    }
    const col = await exportPostman(draft.value.id);
    const blob = new Blob([JSON.stringify(col, null, 2)], {
      type: "application/json",
    });
    saveAs(blob, `${draft.value.name || "connector"}.postman.json`);
  }

  async function openPublish() {
    if (!canWriteWorkbench.value) return;
    if (!canPublish.value) {
      ElMessage.warning($t("upstreamConnector.publishNeedRun"));
      return;
    }
    if (blockIfUnresolved(unresolvedAll.value)) return;
    publishResult.value = null;
    publishChannels.value = autoChannelsFromEndpoints(endpoints.value);
    if (draft.value?.name) publishForm.value.supplier_name = draft.value.name;
    const cryptoMsgs = cryptoIssuesOnEndpoints();
    if (cryptoMsgs.length) {
      ElMessage.warning(cryptoMsgs[0]!);
      return;
    }
    try {
      await saveDraft({ silent: true });
    } catch {
      ElMessage.error($t("upstreamConnector.saveDraftFailed"));
      return;
    }
    showPublish.value = true;
  }

  async function doPublish() {
    if (!canWriteWorkbench.value || !draft.value) return;
    if (blockIfUnresolved(unresolvedAll.value)) return;
    const cryptoMsgsPublish = cryptoIssuesOnEndpoints();
    if (cryptoMsgsPublish.length) {
      ElMessage.warning(cryptoMsgsPublish[0]!);
      return;
    }
    if (!draft.value.name.trim() || !draft.value.upstream_url.trim()) {
      ElMessage.warning($t("upstreamConnector.fieldsRequired"));
      return;
    }
    if (
      !publishForm.value.supplier_code.trim() ||
      !publishForm.value.supplier_name.trim()
    ) {
      ElMessage.warning($t("upstreamConnector.publishFieldsRequired"));
      return;
    }
    publishing.value = true;
    publishResult.value = null;
    try {
      await saveDraft({ silent: true });
      const r = await publishDraft(draft.value.id, {
        mode: publishForm.value.mode,
        supplier: {
          supplier_code: publishForm.value.supplier_code,
          name: publishForm.value.supplier_name,
          status: "active",
        },
        channels:
          publishForm.value.mode === "supplier_only"
            ? undefined
            : publishChannels.value.map((c) => ({
                endpoint_id: c.endpoint_id,
                channel_code: c.channel_code,
                name: c.name,
                path_prefix: c.path_prefix,
                cost_price: c.cost_price,
                qps_limit: c.qps_limit,
              })),
        route:
          publishForm.value.mode === "full_onboarding"
            ? {
                product_ref: { product_id: publishForm.value.product_id },
                priority: 100,
                weight: 100,
                version_tag: "v1.0",
              }
            : undefined,
        options: {
          require_gate: publishForm.value.require_gate,
          force: {
            force_publish: publishForm.value.force_publish,
            force_reason: publishForm.value.force_reason || null,
          },
          fail_if_route_exists: true,
        },
        comment: publishForm.value.comment,
      });
      publishResult.value = r;
      ElMessage.success($t("upstreamConnector.publishOk"));
      if (r.supplier_id && draft.value) {
        draft.value.linked_supplier_id = r.supplier_id;
        await syncSessionRoute(draft.value);
      }
    } finally {
      publishing.value = false;
    }
  }

  function goBack() {
    if (draft.value?.linked_supplier_id) {
      void router.push({
        name: "SupplierDetail",
        params: { id: draft.value.linked_supplier_id },
      });
      return;
    }
    void router.push({
      path: "/operations/upstream-supply",
      query: { tab: "suppliers" },
    });
  }

  async function bootstrap() {
    bootstrapping.value = true;
    try {
      const supplierId =
        typeof route.query.supplier_id === "string"
          ? route.query.supplier_id
          : "";
      const sessionId =
        typeof route.query.session === "string" ? route.query.session : "";

      if (supplierId) {
        const sup = await fetchSupplierById(supplierId);
        if (resolveUpstreamType(sup) === "microservice") {
          leaveForMicroservice(supplierId);
          return;
        }
        if (!canWriteWorkbench.value) {
          const d = await previewDraftFromSupplier(supplierId);
          await openSession(d, sup);
          return;
        }
        const d = await draftFromSupplier(supplierId);
        await openSession(d, sup);
        return;
      }
      if (sessionId) {
        const d = await getDraft(sessionId);
        await openSession(d);
        return;
      }
      if (!canWriteWorkbench.value) {
        draft.value = null;
        void refreshSwitcherOptions();
        return;
      }
      await createBlankSession();
    } catch (e) {
      draft.value = null;
      ElMessage.error(
        (e as Error)?.message || $t("upstreamConnector.bootstrapFailed"),
      );
    } finally {
      bootstrapping.value = false;
    }
  }

  function selectHistoryRun(run: ConnectorRun) {
    const ep = endpoints.value.find(
      (e) =>
        run.endpoint_key &&
        (e.id === run.endpoint_key ||
          e.name === run.endpoint_key ||
          e.path === run.endpoint_key),
    );
    if (ep?.id) {
      openEndpointTab(ep.id);
      ensureTabRuntime(ep.id).lastRun = run;
    } else if (activeEp.value?.id) {
      ensureTabRuntime(activeEp.value.id).lastRun = run;
    }
    responseCollapsed.value = false;
  }

  return {
    // RBAC
    isSuperAdmin,
    canWriteWorkbench,
    canForcePublish,

    // Layout
    activeSidebarPanel,
    sidebarCollapsed,
    responseCollapsed,
    responseSplitDir,
    sidebarSize,
    requestSize,

    // Session & Switcher
    draft,
    recentDrafts,
    transparentSuppliers,
    switcherLoading,
    refreshSwitcherOptions,
    switchToDraft,
    switchToSupplier,
    createBlankSession,
    bootstrapping,
    saving,
    dirty,
    saveError,
    saveBadge,
    isSupplierLinked,
    leavePromptOpen,
    shouldGuardLeave,
    saveDraft,
    saveDraftExplicit,
    resyncFromLiveSupplier,
    goBack,
    bootstrap,

    // Multi-Tab & Endpoints
    endpoints,
    openTabIds,
    openTabs,
    activeTabId,
    activeEpIdx,
    activeEp,
    activeRuntime,
    reqTab,
    resTab,
    running,
    lastRun,
    runs,
    openEndpointTab,
    closeEndpointTab,
    switchEndpointByIndex,
    addEndpoint,
    onEpAction,
    onEpReorder,
    toggleGate,
    selectHistoryRun,

    // KV, URL & Auth/Transform/TLS
    headerRows,
    queryRows,
    gqlVarRows,
    gqlEnumRows,
    urlDraft,
    urlFocused,
    urlMissing,
    uploadingArtifact,
    configForm,
    supplierTimeoutMs,
    supplierTlsForm,
    endpointTlsForm,
    activeInvalidationSource,
    activeAssertions,
    activeCaptures,
    sessionCredentialKeys,
    authSeedJson,
    queryCount,
    headerCount,
    captureCount,
    bodyFilled,
    authFilled,
    transformFilled,
    assertFilled,
    tlsFilled,
    setEpAuthProfile,
    onCryptoProfileChange,
    onBodyEncodingChange,
    onAdapterQuickSelect,
    onUrlInput,
    onUrlBlur,
    refreshUrlDraft,
    applyCurlPaste,
    copyCurl,
    onArtifactFile,
    flushKvToActive,

    // Multi-Environment & Captures
    envState,
    activeNamedEnv,
    capturedVars,
    capturedCount,
    envVarCount,
    staticEnvMap,
    runtimeEnvMap,
    unresolved,
    unresolvedAll,
    activeStaticCaptureWarnings,
    addNamedEnv,
    deleteNamedEnv,
    setActiveEnv,
    exportEnvToFile,
    importEnvFromText,
    clearCapturedVars,
    removeCapturedVar,

    // Execution
    doRun,
    doLiveRun,
    doTransformPreview,
    abortActiveRun,
    abortAllRuns,

    // Import / Export / Publish
    importing,
    publishing,
    showImport,
    showPublish,
    showCommands,
    showShortcuts,
    importTab,
    importMode,
    importCurlText,
    importJsonText,
    publishForm,
    publishChannels,
    publishResult,
    publishResultDesc,
    canPublish,
    gate,
    leftoverTokens,
    doImport,
    doExport,
    openPublish,
    doPublish,
  };
}
