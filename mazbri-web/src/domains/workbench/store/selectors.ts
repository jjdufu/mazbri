import type {
  ConnectorEndpoint,
  ConnectorRun,
  DraftSnapshotPayload,
} from "@/api/upstream-connectors";
import { $t } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import { useLocaleStore } from "@/stores/locale";
import { canWriteUpstreamConnector, isAdminRole } from "@/utils/ops/roles";
import { isMaskedSecretValue } from "@/utils/originPinning";
import { serializeAuthFlow } from "@/domains/upstream-supply/supplier-form";
import { isSessionHttpAuthActive } from "../engine/authMatrix";
import {
  CODE_FREE_UPSTREAM_TYPE,
  entriesToKv,
  findStaticCapturePlaceholders,
  findUnresolvedVars,
  gateViewFromServer,
  isPersistedDraftId,
  kvToMap,
  leftoverTokenHeaderNames,
  mapToKv,
  mergeEnvWithCaptures,
  recordFromUnknown,
  resolveActiveEnvMap,
  sanitizeAuthSpec,
  serializeCaptureList,
  serializeWorkbenchEnvDoc,
  tlsFormToSpec,
  workbenchEnvSurfaceParts,
  type GateEpStatus,
  type WorkbenchNamedEnvState,
} from "../engine/workbenchHelpers";
import type {
  EndpointTabRuntime,
  OpenEndpointTab,
  SaveBadgeState,
  WorkbenchState,
} from "./types";

export const DEFAULT_TAB_RUNTIME: EndpointTabRuntime = Object.freeze({
  reqTab: "params",
  resTab: "json",
  running: false,
  lastRun: null,
  runGen: 0,
});

function createMemoizedSelector<Args extends unknown[], R>(
  extract: (state: WorkbenchState) => readonly [...Args],
  compute: (...args: Args) => R,
): (state: WorkbenchState) => R {
  let hasCache = false;
  let lastArgs: readonly [...Args] | null = null;
  let lastResult: R;
  return (state: WorkbenchState): R => {
    const nextArgs = extract(state);
    if (
      hasCache &&
      lastArgs &&
      lastArgs.length === nextArgs.length &&
      nextArgs.every((v, i) => Object.is(v, lastArgs![i]))
    ) {
      return lastResult;
    }
    lastArgs = nextArgs;
    lastResult = compute(...nextArgs);
    hasCache = true;
    return lastResult;
  };
}

export function formToApiConfigFromState(
  state: Pick<
    WorkbenchState,
    "configForm" | "supplierTimeoutMs" | "supplierTlsForm"
  >,
): Record<string, unknown> {
  const credsMap: Record<string, string> = {};
  for (const item of state.configForm.auth_flow.credentials) {
    if (item.key.trim()) {
      credsMap[item.key.trim()] = item.value;
    }
  }
  const hasCreds = Object.keys(credsMap).length > 0;
  const httpActive = isSessionHttpAuthActive(
    state.configForm.auth_flow.enabled,
    state.configForm.auth_flow.profile,
  );
  const payload: Record<string, unknown> = {
    upstream_type: CODE_FREE_UPSTREAM_TYPE,
    auth_flow: httpActive
      ? serializeAuthFlow(state.configForm.auth_flow, credsMap)
      : hasCreds
        ? { enabled: false, profile: "none", credentials: credsMap }
        : null,
    timeouts: {
      total_ms:
        Number.isFinite(state.supplierTimeoutMs) && state.supplierTimeoutMs > 0
          ? Math.round(state.supplierTimeoutMs)
          : 15000,
    },
  };
  const tls = tlsFormToSpec(state.supplierTlsForm);
  if (tls) payload.tls = tls;
  return payload;
}

export function projectEndpointsFromState(
  state: Pick<
    WorkbenchState,
    | "endpoints"
    | "activeTabId"
    | "headerRows"
    | "queryRows"
    | "gqlVarRows"
    | "gqlEnumRows"
    | "endpointTlsForm"
  >,
): ConnectorEndpoint[] {
  const activeEp = selectActiveEp(state);
  const activeId = activeEp?.id || "";
  return state.endpoints.map((ep) => {
    const isActive = Boolean(activeId) && ep.id === activeId;
    const hRows = isActive
      ? state.headerRows
      : entriesToKv(ep.header_entries, ep.headers || {});
    const qRows = isActive
      ? state.queryRows
      : entriesToKv(ep.query_entries, ep.query || {});
    const tlsVal = isActive
      ? tlsFormToSpec(state.endpointTlsForm)
      : (ep.tls ?? undefined);
    const nextEp: ConnectorEndpoint = {
      ...ep,
      header_entries: hRows.map((r) => ({
        key: r.key,
        value: r.value,
        desc: r.desc ?? "",
        enabled: r.enabled !== false,
      })),
      query_entries: qRows.map((r) => ({
        key: r.key,
        value: r.value,
        desc: r.desc ?? "",
        enabled: r.enabled !== false,
      })),
      headers: kvToMap(hRows),
      query: kvToMap(qRows),
      tls: tlsVal,
      captures: serializeCaptureList(ep.captures),
      assertions: (ep.assertions || []).map(({ uid: _uid, ...rest }) => rest),
    };
    if (nextEp.adapter === "graphql") {
      const vRows = isActive
        ? state.gqlVarRows
        : mapToKv(recordFromUnknown(ep.graphql?.variables_map));
      const eRows = isActive
        ? state.gqlEnumRows
        : mapToKv(recordFromUnknown(ep.graphql?.enum_map));
      nextEp.graphql = {
        ...(ep.graphql || {}),
        variables_map: kvToMap(vRows),
        enum_map: kvToMap(eRows),
      };
    }
    if (nextEp.auth) {
      const nextAuth = sanitizeAuthSpec(
        nextEp.auth as Record<string, unknown>,
      );
      nextEp.auth = nextAuth || { profile: "none" };
    }
    return nextEp;
  });
}

export function buildDraftSnapshotFromState(
  state: WorkbenchState,
): DraftSnapshotPayload {
  return {
    name: state.draft?.name ?? "",
    upstream_type: CODE_FREE_UPSTREAM_TYPE,
    upstream_url: state.draft?.upstream_url ?? "",
    config: formToApiConfigFromState(state),
    endpoints: projectEndpointsFromState(state),
    workbench_env: serializeWorkbenchEnvDoc(state.envState),
  };
}

export function computeSnapshotSig(state: WorkbenchState): string {
  if (!state.draft) return "";
  return JSON.stringify(buildDraftSnapshotFromState(state));
}

export function staticEndpointGateSigFromState(
  state: Pick<
    WorkbenchState,
    "draft" | "configForm" | "supplierTimeoutMs" | "supplierTlsForm" | "envState"
  >,
  ep: ConnectorEndpoint,
): string {
  const {
    captures: _c,
    assertions: _a,
    channel_code: _cc,
    path_prefix: _pp,
    cost_price: _cp,
    qps_limit: _ql,
    tag: _tg,
    name: _nm,
    ...staticRest
  } = ep;
  return JSON.stringify({
    upstream_url: state.draft?.upstream_url ?? "",
    config: formToApiConfigFromState(state),
    env: resolveActiveEnvMap(state.envState),
    ep: staticRest,
  });
}

export function selectCanWriteWorkbench(_state?: WorkbenchState): boolean {
  return canWriteUpstreamConnector(useAuthStore.getState().info.role);
}

export function selectIsSuperAdmin(_state?: WorkbenchState): boolean {
  const info = useAuthStore.getState().info;
  return Boolean(info.isSuperAdmin || isAdminRole(info.role));
}

export function selectActiveEpIdx(
  state: Pick<WorkbenchState, "endpoints" | "activeTabId">,
): number {
  if (!state.endpoints.length) return -1;
  const idx = state.endpoints.findIndex((ep) => ep.id === state.activeTabId);
  return idx >= 0 ? idx : 0;
}

export function selectActiveEp(
  state: Pick<WorkbenchState, "endpoints" | "activeTabId">,
): ConnectorEndpoint | null {
  const idx = selectActiveEpIdx(state);
  return idx >= 0 ? (state.endpoints[idx] ?? null) : null;
}

export function selectActiveRuntime(
  state: Pick<WorkbenchState, "endpoints" | "activeTabId" | "tabRuntimeMap">,
): EndpointTabRuntime {
  const ep = selectActiveEp(state);
  const id = ep?.id || "";
  if (!id) return DEFAULT_TAB_RUNTIME;
  return state.tabRuntimeMap[id] ?? DEFAULT_TAB_RUNTIME;
}

export const selectOpenTabs = createMemoizedSelector(
  (state) => [state.openTabIds, state.endpoints, state.tabRuntimeMap] as const,
  (openTabIds, endpoints, tabRuntimeMap): OpenEndpointTab[] => {
    const out: OpenEndpointTab[] = [];
    for (const id of openTabIds) {
      const index = endpoints.findIndex((ep) => ep.id === id);
      if (index < 0) continue;
      out.push({
        id,
        index,
        ep: endpoints[index]!,
        runtime: tabRuntimeMap[id] ?? DEFAULT_TAB_RUNTIME,
      });
    }
    return out;
  },
);

export const selectStaticEnvMap = createMemoizedSelector(
  (state) => [state.envState] as const,
  (envState): Record<string, string> => resolveActiveEnvMap(envState),
);

export const selectRuntimeEnvMap = createMemoizedSelector(
  (state) => [state.envState, state.capturedVars] as const,
  (envState, capturedVars): Record<string, string> =>
    mergeEnvWithCaptures(resolveActiveEnvMap(envState), capturedVars),
);

export function selectActiveNamedEnv(
  state: Pick<WorkbenchState, "envState">,
): WorkbenchNamedEnvState | null {
  return (
    state.envState.environments.find(
      (e) => e.id === state.envState.activeEnvId,
    ) ?? null
  );
}

export const selectGateView = createMemoizedSelector(
  (state) =>
    [
      state.draft,
      state.endpoints,
      state.activeTabId,
      state.headerRows,
      state.queryRows,
      state.gqlVarRows,
      state.gqlEnumRows,
      state.endpointTlsForm,
      state.configForm,
      state.supplierTimeoutMs,
      state.supplierTlsForm,
      state.envState,
      state.passedStaticSigMap,
    ] as const,
  (
    draft,
    endpoints,
    activeTabId,
    headerRows,
    queryRows,
    gqlVarRows,
    gqlEnumRows,
    endpointTlsForm,
    configForm,
    supplierTimeoutMs,
    supplierTlsForm,
    envState,
    passedStaticSigMap,
  ): { ok: number; total: number; items: GateEpStatus[] } => {
    const projected = projectEndpointsFromState({
      endpoints,
      activeTabId,
      headerRows,
      queryRows,
      gqlVarRows,
      gqlEnumRows,
      endpointTlsForm,
    });
    const sigState = {
      draft,
      configForm,
      supplierTimeoutMs,
      supplierTlsForm,
      envState,
    };
    const adjustedServer = (draft?.gate || []).map((g) => {
      if (!g.met) return g;
      const idx = projected.findIndex(
        (ep, i) =>
          g.endpoint_key === ep.id ||
          g.endpoint_key === ep.name ||
          g.endpoint_key === ep.path ||
          g.endpoint_key === `ep-${i + 1}`,
      );
      if (idx < 0) return g;
      const ep = projected[idx]!;
      const id = ep.id || `ep-${idx + 1}`;
      const passedSig = passedStaticSigMap[id];
      if (passedSig && passedSig !== staticEndpointGateSigFromState(sigState, ep)) {
        return { ...g, met: false, reason: "spec_changed" as const };
      }
      return g;
    });
    return gateViewFromServer(endpoints, adjustedServer);
  },
);

export const selectLeftoverTokens = createMemoizedSelector(
  (state) => [state.endpoints, state.configForm] as const,
  (endpoints, configForm): string[] =>
    leftoverTokenHeaderNames({
      endpoints,
      authEnabled: isSessionHttpAuthActive(
        configForm.auth_flow.enabled,
        configForm.auth_flow.profile,
      ),
      injectTo: configForm.auth_flow.inject_to,
      injectKey: configForm.auth_flow.inject_key,
    }),
);

export function selectDirty(state: WorkbenchState): boolean {
  if (!selectCanWriteWorkbench() || !state.draft || state.bootstrapping) {
    return false;
  }
  return computeSnapshotSig(state) !== state.savedSnapshotSig;
}

function isSuccessfulSend(r: ConnectorRun | null | undefined): boolean {
  return Boolean(r?.ok) && r?.mode !== "transform_preview";
}

export function selectCanPublish(state: WorkbenchState): boolean {
  const activeRun = selectActiveRuntime(state).lastRun;
  return (
    state.runs.some((r) => isSuccessfulSend(r)) || isSuccessfulSend(activeRun)
  );
}

export const selectUnresolvedRun = createMemoizedSelector(
  (state) =>
    [
      state.draft?.upstream_url,
      state.endpoints,
      state.activeTabId,
      state.configForm,
      state.envState,
      state.capturedVars,
    ] as const,
  (
    upstreamUrl,
    endpoints,
    activeTabId,
    configForm,
    envState,
    capturedVars,
  ): string[] => {
    const ep = selectActiveEp({ endpoints, activeTabId });
    if (!ep) return [];
    const runtimeEnv = mergeEnvWithCaptures(
      resolveActiveEnvMap(envState),
      capturedVars,
    );
    return findUnresolvedVars(
      workbenchEnvSurfaceParts({
        upstreamUrl,
        endpoint: ep,
        authFlow: configForm.auth_flow,
      }),
      runtimeEnv,
    );
  },
);

function unresolvedForPublish(
  upstreamUrl: string | undefined,
  ep: ConnectorEndpoint | null | undefined,
  authFlow: WorkbenchState["configForm"]["auth_flow"],
  staticEnv: Record<string, string>,
): string[] {
  if (!ep) return [];
  const staticCaptures = findStaticCapturePlaceholders({
    upstreamUrl,
    endpoint: ep,
    authFlow,
  });
  const staticUnresolved = findUnresolvedVars(
    workbenchEnvSurfaceParts({
      upstreamUrl,
      endpoint: { ...ep, body: undefined },
      authFlow,
    }),
    staticEnv,
  );
  return [...new Set([...staticCaptures, ...staticUnresolved])];
}

export const selectUnresolvedAll = createMemoizedSelector(
  (state) =>
    [
      state.draft?.upstream_url,
      state.endpoints,
      state.configForm,
      state.envState,
    ] as const,
  (upstreamUrl, endpoints, configForm, envState): string[] => {
    const staticEnv = resolveActiveEnvMap(envState);
    const names = new Set<string>();
    for (const ep of endpoints) {
      for (const n of unresolvedForPublish(
        upstreamUrl,
        ep,
        configForm.auth_flow,
        staticEnv,
      )) {
        names.add(n);
      }
    }
    if (!endpoints.length) {
      for (const n of unresolvedForPublish(
        upstreamUrl,
        { method: "GET", path: "/" },
        configForm.auth_flow,
        staticEnv,
      )) {
        names.add(n);
      }
    }
    return [...names];
  },
);

export const selectActiveStaticCaptureWarnings = createMemoizedSelector(
  (state) =>
    [
      state.draft?.upstream_url,
      state.endpoints,
      state.activeTabId,
      state.configForm,
    ] as const,
  (upstreamUrl, endpoints, activeTabId, configForm): string[] => {
    const ep = selectActiveEp({ endpoints, activeTabId });
    return findStaticCapturePlaceholders({
      upstreamUrl,
      endpoint: ep,
      authFlow: configForm.auth_flow,
    });
  },
);

export const selectRequiredCredKeys = createMemoizedSelector(
  (state) => [state.originClearedCredKeys, state.configForm] as const,
  (originClearedCredKeys, configForm): string[] => {
    if (!originClearedCredKeys.length) return [];
    const creds = configForm.auth_flow.credentials || [];
    return originClearedCredKeys.filter((reqKey) => {
      const hit = creds.find((c) => c.key.trim() === reqKey);
      return (
        Boolean(hit) && (!hit!.value.trim() || isMaskedSecretValue(hit!.value))
      );
    });
  },
);

export const selectSessionCredentialKeys = createMemoizedSelector(
  (state) => [state.configForm] as const,
  (configForm): string[] =>
    configForm.auth_flow.credentials
      .map((item) => item.key.trim())
      .filter(Boolean),
);

export const selectSaveBadge = createMemoizedSelector(
  (state) =>
    [
      state.saving,
      state.saveError,
      state.lastSavedAt,
      state.draft?.id,
      selectDirty(state),
      useLocaleStore.getState().locale,
    ] as const,
  (saving, saveError, lastSavedAt, draftId, dirty, _locale): SaveBadgeState => {
    if (saving) {
      return { type: "info", text: $t("upstreamConnector.savingDraft") };
    }
    if (saveError) {
      return { type: "danger", text: $t("upstreamConnector.saveDraftFailed") };
    }
    if (dirty) {
      return { type: "warning", text: $t("upstreamConnector.unsaved") };
    }
    if (lastSavedAt && isPersistedDraftId(draftId)) {
      return { type: "success", text: $t("upstreamConnector.saved") };
    }
    return null;
  },
);

export function selectPublishResultDesc(
  state: Pick<WorkbenchState, "publishResult">,
): string {
  if (!state.publishResult) return "";
  const r = state.publishResult;
  return `supplier=${r.supplier_id} version=v${r.version_no} route_version=${r.route_version ?? "—"}`;
}
