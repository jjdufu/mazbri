import type {
  ConnectorDraft,
  ConnectorEndpoint,
  ConnectorRun,
  PublishResult,
} from "@/api/upstream-connectors";
import type { Supplier } from "@/api/supplier";
import type { SupplierConfigForm } from "@/domains/upstream-supply/supplier-form";
import type {
  ChannelDraft,
  KvRow,
  ParsedImport,
  SplitDirection,
  TlsFormState,
  WorkbenchEnvState,
} from "../engine/workbenchHelpers";

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
export type PublishMode =
  | "supplier_only"
  | "supplier_channels"
  | "full_onboarding";
export type ImportTabId = "curl" | "postman" | "openapi";
export type ImportMode = "merge" | "replace";

export type WorkbenchNavigate = (
  to: string,
  opts?: { replace?: boolean },
) => void;

export interface EndpointTabRuntime {
  reqTab: RequestSubTabId;
  resTab: ResponseSubTabId;
  running: boolean;
  lastRun: ConnectorRun | null;
  runGen: number;
}

export interface OpenEndpointTab {
  id: string;
  index: number;
  ep: ConnectorEndpoint;
  runtime: EndpointTabRuntime;
}

export interface PublishFormState {
  mode: PublishMode;
  supplier_code: string;
  supplier_name: string;
  product_id: string;
  require_gate: boolean;
  force_publish: boolean;
  force_reason: string;
  comment: string;
}

export type SaveBadgeState = {
  type: "info" | "danger" | "warning" | "success";
  text: string;
} | null;

export interface WorkbenchConfirmState {
  open: boolean;
  title: string;
  description: string;
  confirmLabel?: string;
  danger?: boolean;
  resolve: (ok: boolean) => void;
}

export interface WorkbenchState {
  // Layout
  activeSidebarPanel: SidebarPanelId;
  sidebarCollapsed: boolean;
  responseCollapsed: boolean;
  responseSplitDir: SplitDirection;
  envDrawerOpen: boolean;

  // Session
  draft: ConnectorDraft | null;
  recentDrafts: ConnectorDraft[];
  transparentSuppliers: Supplier[];
  switcherLoading: boolean;
  bootstrapping: boolean;
  saving: boolean;
  saveError: boolean;
  lastSavedAt: number | null;
  savedSnapshotSig: string;
  isEditMode: boolean;
  endpoints: ConnectorEndpoint[];
  openTabIds: string[];
  activeTabId: string;
  tabRuntimeMap: Record<string, EndpointTabRuntime>;
  runs: ConnectorRun[];
  envState: WorkbenchEnvState;
  capturedVars: Record<string, string>;

  // Editor buffers
  headerRows: KvRow[];
  queryRows: KvRow[];
  gqlVarRows: KvRow[];
  gqlEnumRows: KvRow[];
  urlDraft: string;
  urlFocused: boolean;
  urlMissing: boolean;
  uploadingArtifact: boolean;
  configForm: SupplierConfigForm;
  supplierTimeoutMs: number;
  supplierTlsForm: TlsFormState;
  endpointTlsForm: TlsFormState;
  originClearedCredKeys: string[];
  passedStaticSigMap: Record<string, string>;

  // Dialogs
  importing: boolean;
  publishing: boolean;
  showImport: boolean;
  showPublish: boolean;
  showCommands: boolean;
  showShortcuts: boolean;
  importTab: ImportTabId;
  importMode: ImportMode;
  importCurlText: string;
  importJsonText: string;
  publishForm: PublishFormState;
  publishChannels: ChannelDraft[];
  publishResult: PublishResult | null;
  confirmState: WorkbenchConfirmState | null;
}

export interface WorkbenchActions {
  setSidebarPanel: (panel: SidebarPanelId) => void;
  setSidebarCollapsed: (collapsed: boolean) => void;
  toggleSidebar: () => void;
  openSidebarTo: (panel: SidebarPanelId) => void;
  setResponseCollapsed: (collapsed: boolean) => void;
  toggleResponseCollapsed: () => void;
  toggleSplitDirection: () => void;
  setEnvDrawerOpen: (open: boolean) => void;
  setShowImport: (open: boolean) => void;
  setShowPublish: (open: boolean) => void;
  setShowCommands: (open: boolean) => void;
  setShowShortcuts: (open: boolean) => void;
  setImportTab: (tab: ImportTabId) => void;
  setImportMode: (mode: ImportMode) => void;
  setImportCurlText: (text: string) => void;
  setImportJsonText: (text: string) => void;
  updatePublishForm: (patch: Partial<PublishFormState>) => void;
  setPublishChannels: (channels: ChannelDraft[]) => void;
  resolveConfirm: (ok: boolean) => void;
  setReqTab: (tab: RequestSubTabId) => void;
  setResTab: (tab: ResponseSubTabId) => void;
  updateDraft: (patch: Partial<ConnectorDraft>) => void;
  updateActiveEndpoint: (
    updater: (ep: ConnectorEndpoint) => ConnectorEndpoint,
  ) => void;
  updateEndpointAt: (
    index: number,
    updater: (ep: ConnectorEndpoint) => ConnectorEndpoint,
  ) => void;
  setHeaderRows: (rows: KvRow[]) => void;
  setQueryRows: (rows: KvRow[]) => void;
  setGqlVarRows: (rows: KvRow[]) => void;
  setGqlEnumRows: (rows: KvRow[]) => void;
  setUrlFocused: (focused: boolean) => void;
  onUrlInput: (raw: string) => void;
  onUrlBlur: () => void;
  applyCurlPaste: (parsed: ParsedImport) => void;
  setConfigForm: (
    updater: (prev: SupplierConfigForm) => SupplierConfigForm,
  ) => void;
  setSupplierTimeoutMs: (ms: number) => void;
  setSupplierTlsForm: (form: TlsFormState) => void;
  setEndpointTlsForm: (form: TlsFormState) => void;
  setEpAuthProfile: (profile: string) => void;
  onCryptoProfileChange: (value: Record<string, unknown> | null) => void;
  onBodyEncodingChange: (value: string) => void;
  onAdapterQuickSelect: (adapter: string) => void;
  uploadArtifactFile: (file: File) => Promise<void>;
  openEndpointTab: (epId: string) => void;
  closeEndpointTab: (epId: string) => void;
  switchEndpointByIndex: (index: number) => void;
  addEndpoint: (defaultTag?: string) => void;
  duplicateEndpoint: (index: number) => void;
  deleteEndpoint: (index: number) => void;
  toggleEndpointGate: (index: number) => void;
  setEnvState: (
    updater: (prev: WorkbenchEnvState) => WorkbenchEnvState,
  ) => void;
  setActiveEnv: (id: string | null) => void;
  addNamedEnv: (name: string) => void;
  deleteNamedEnv: (id: string) => void;
  exportEnvToFile: () => void;
  importEnvFromText: (rawText: string) => void;
  clearCapturedVars: () => void;
  removeCapturedVar: (name: string) => void;
  selectHistoryRun: (run: ConnectorRun) => void;
  refreshSwitcherOptions: () => Promise<void>;
  bootstrap: (
    params: { supplierId?: string; sessionId?: string },
    navigate: WorkbenchNavigate,
  ) => Promise<void>;
  createBlankSession: (navigate?: WorkbenchNavigate) => Promise<void>;
  switchToDraft: (
    draftId: string,
    navigate?: WorkbenchNavigate,
  ) => Promise<void>;
  switchToSupplier: (
    supplierId: string,
    navigate?: WorkbenchNavigate,
  ) => Promise<void>;
  saveDraft: (opts?: {
    silent?: boolean;
    skipRouteSync?: boolean;
    navigate?: WorkbenchNavigate;
  }) => Promise<void>;
  saveDraftExplicit: (navigate?: WorkbenchNavigate) => Promise<void>;
  resyncFromLiveSupplier: (navigate?: WorkbenchNavigate) => Promise<void>;
  doRun: (mode: string, focusUrl?: () => void) => Promise<void>;
  doLiveRun: () => Promise<void>;
  doTransformPreview: () => Promise<void>;
  abortActiveRun: () => void;
  abortAllRuns: () => void;
  copyCurl: () => Promise<void>;
  doImport: (navigate?: WorkbenchNavigate) => Promise<void>;
  doExport: (navigate?: WorkbenchNavigate) => Promise<void>;
  openPublish: (navigate?: WorkbenchNavigate) => Promise<void>;
  doPublish: (navigate?: WorkbenchNavigate) => Promise<void>;
  confirmDiscardIfDirty: () => Promise<boolean>;
  resetStore: () => void;
}

export type WorkbenchStore = WorkbenchState & WorkbenchActions;
