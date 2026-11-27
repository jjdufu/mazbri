import { useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router";
import {
  ArrowLeft,
  ChevronDown,
  Command,
  Download,
  FileJson,
  Languages,
  Moon,
  Plus,
  RefreshCw,
  Save,
  SlidersHorizontal,
  Sun,
  Upload,
  UploadCloud,
} from "lucide-react";
import {
  Badge,
  Button,
  IconButton,
  Input,
  Popover,
  Tooltip,
  cn,
} from "@/components/ui";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import { useThemeStore } from "@/stores/theme";
import { canWriteUpstreamConnector } from "@/utils/ops/roles";
import {
  selectActiveNamedEnv,
  selectDirty,
  selectGateView,
  selectRuntimeEnvMap,
  selectSaveBadge,
  selectStaticEnvMap,
  useWorkbenchStore,
} from "../store";

export interface WorkbenchTopBarProps {
  onBack: () => void;
  onOpenEnvDrawer: () => void;
}

export function WorkbenchTopBar({
  onBack,
  onOpenEnvDrawer,
}: WorkbenchTopBarProps) {
  const { t, toggleLocale } = useT();
  const navigate = useNavigate();
  const role = useAuthStore((s) => s.info.role);
  const canWrite = canWriteUpstreamConnector(role);
  const isDark = useThemeStore((s) => s.isDark);
  const toggleTheme = useThemeStore((s) => s.toggleTheme);

  const draft = useWorkbenchStore((s) => s.draft);
  const recentDrafts = useWorkbenchStore((s) => s.recentDrafts);
  const transparentSuppliers = useWorkbenchStore((s) => s.transparentSuppliers);
  const isEditMode = useWorkbenchStore((s) => s.isEditMode);
  const saving = useWorkbenchStore((s) => s.saving);
  const publishing = useWorkbenchStore((s) => s.publishing);
  const publishForm = useWorkbenchStore((s) => s.publishForm);
  const envState = useWorkbenchStore((s) => s.envState);

  const activeNamedEnv = useWorkbenchStore(selectActiveNamedEnv);
  const staticEnvMap = useWorkbenchStore(selectStaticEnvMap);
  const runtimeEnvMap = useWorkbenchStore(selectRuntimeEnvMap);
  const gate = useWorkbenchStore(selectGateView);
  const saveBadge = useWorkbenchStore(selectSaveBadge);
  const dirty = useWorkbenchStore(selectDirty);

  const updateDraft = useWorkbenchStore((s) => s.updateDraft);
  const refreshSwitcherOptions = useWorkbenchStore(
    (s) => s.refreshSwitcherOptions,
  );
  const createBlankSession = useWorkbenchStore((s) => s.createBlankSession);
  const switchToDraft = useWorkbenchStore((s) => s.switchToDraft);
  const switchToSupplier = useWorkbenchStore((s) => s.switchToSupplier);
  const resyncFromLiveSupplier = useWorkbenchStore(
    (s) => s.resyncFromLiveSupplier,
  );
  const setActiveEnv = useWorkbenchStore((s) => s.setActiveEnv);
  const addNamedEnv = useWorkbenchStore((s) => s.addNamedEnv);
  const setShowCommands = useWorkbenchStore((s) => s.setShowCommands);
  const setShowImport = useWorkbenchStore((s) => s.setShowImport);
  const doExport = useWorkbenchStore((s) => s.doExport);
  const exportEnvToFile = useWorkbenchStore((s) => s.exportEnvToFile);
  const importEnvFromText = useWorkbenchStore((s) => s.importEnvFromText);
  const saveDraftExplicit = useWorkbenchStore((s) => s.saveDraftExplicit);
  const openSidebarTo = useWorkbenchStore((s) => s.openSidebarTo);
  const openPublish = useWorkbenchStore((s) => s.openPublish);

  const [switcherOpen, setSwitcherOpen] = useState(false);
  const [envMenuOpen, setEnvMenuOpen] = useState(false);
  const [ioMenuOpen, setIoMenuOpen] = useState(false);
  const [newEnvName, setNewEnvName] = useState("");
  const envFileInputRef = useRef<HTMLInputElement | null>(null);

  const isSupplierLinked = Boolean(draft?.linked_supplier_id || isEditMode);
  const envVarCount = useMemo(
    () => Object.keys(staticEnvMap).length,
    [staticEnvMap],
  );
  const runtimeEntries = useMemo(
    () => Object.entries(runtimeEnvMap),
    [runtimeEnvMap],
  );
  const isGateReady = gate.total > 0 && gate.ok >= gate.total;

  const badgeVariant =
    saveBadge?.type === "info"
      ? "xblue"
      : saveBadge?.type === "danger"
        ? "danger"
        : saveBadge?.type === "warning"
          ? "warning"
          : saveBadge?.type === "success"
            ? "success"
            : "neutral";

  const handleSubmitNewEnv = () => {
    const trimmed = newEnvName.trim();
    if (!trimmed) return;
    addNamedEnv(trimmed);
    setNewEnvName("");
    setEnvMenuOpen(false);
  };

  const handleEnvFilePicked = async (
    ev: React.ChangeEvent<HTMLInputElement>,
  ) => {
    const file = ev.target.files?.[0];
    ev.target.value = "";
    if (!file) return;
    const text = await file.text();
    importEnvFromText(text);
  };

  return (
    <header className="flex h-11 shrink-0 items-center justify-between gap-2 border-b border-border bg-card px-3">
      {/* Left: Back + Draft name + Session Switcher + Status */}
      <div className="flex min-w-0 flex-1 items-center gap-1.5">
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={onBack}
          title={t("common.actions.back")}
          aria-label={t("common.actions.back")}
          data-testid="wb-topbar-back"
        >
          <ArrowLeft className="h-3.5 w-3.5" />
          <span>{t("common.actions.back")}</span>
        </Button>

        <span className="h-4 w-px shrink-0 bg-border" aria-hidden="true" />

        {draft && (
          <>
            <input
              type="text"
              value={draft.name}
              readOnly={!canWrite}
              onChange={(e) => updateDraft({ name: e.target.value })}
              aria-label={t("upstreamConnector.fields.name")}
              placeholder={t("upstreamConnector.untitledName")}
              data-testid="wb-draft-name-input"
              className={cn(
                "h-8 min-w-[110px] max-w-[220px] rounded-md border border-transparent bg-transparent px-2 text-xs font-semibold text-foreground placeholder:text-muted-foreground transition-colors focus:outline-none",
                canWrite &&
                  "hover:border-border hover:bg-hover-bg focus:border-primary focus:bg-card focus:ring-2 focus:ring-primary/30",
              )}
            />

            <Popover
              open={switcherOpen}
              onOpenChange={(next) => {
                setSwitcherOpen(next);
                if (next) void refreshSwitcherOptions();
              }}
              align="left"
              trigger={
                <button
                  type="button"
                  onClick={() => {
                    const next = !switcherOpen;
                    setSwitcherOpen(next);
                    if (next) void refreshSwitcherOptions();
                  }}
                  title={t("upstreamConnector.switchSession")}
                  aria-label={t("upstreamConnector.switchSession")}
                  data-testid="wb-session-switcher"
                  className="inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md border border-border bg-muted/60 px-2 text-xs font-medium text-muted-foreground hover:bg-hover-bg hover:text-foreground transition-colors cursor-pointer"
                >
                  <span
                    className={cn(
                      "h-1.5 w-1.5 shrink-0 rounded-full",
                      isSupplierLinked
                        ? "bg-success-fg"
                        : "bg-muted-foreground/60",
                    )}
                  />
                  <span className="max-w-[140px] truncate font-mono-tech text-[11px]">
                    {isSupplierLinked
                      ? publishForm.supplier_code ||
                        t("upstreamConnector.statusPublished")
                      : t("upstreamConnector.statusDraft")}
                  </span>
                  <ChevronDown className="h-3.5 w-3.5 shrink-0" />
                </button>
              }
              contentClassName="w-72 p-1.5 space-y-1.5"
            >
              {canWrite && (
                <button
                  type="button"
                  onClick={() => {
                    setSwitcherOpen(false);
                    void createBlankSession(navigate);
                  }}
                  className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-xs font-medium text-primary hover:bg-primary-soft cursor-pointer"
                >
                  <Plus className="h-3.5 w-3.5" />
                  <span>{t("upstreamConnector.newSession")}</span>
                </button>
              )}

              {canWrite && draft.linked_supplier_id && (
                <button
                  type="button"
                  onClick={() => {
                    setSwitcherOpen(false);
                    void resyncFromLiveSupplier(navigate);
                  }}
                  className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-xs text-foreground hover:bg-hover-bg cursor-pointer"
                >
                  <RefreshCw className="h-3.5 w-3.5 text-muted-foreground" />
                  <span>{t("upstreamConnector.resyncFromLive")}</span>
                </button>
              )}

              {recentDrafts.length > 0 && (
                <div className="space-y-0.5 border-t border-border pt-1.5">
                  <div className="px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                    {t("upstreamConnector.recentDrafts")}
                  </div>
                  <div className="max-h-40 overflow-y-auto space-y-0.5">
                    {recentDrafts.slice(0, 10).map((d) => {
                      const isCurrent = d.id === draft.id;
                      return (
                        <button
                          key={d.id}
                          type="button"
                          onClick={() => {
                            setSwitcherOpen(false);
                            void switchToDraft(d.id, navigate);
                          }}
                          className={cn(
                            "flex w-full items-center justify-between gap-2 rounded-md px-2 py-1.5 text-xs text-left cursor-pointer",
                            isCurrent
                              ? "bg-primary-soft text-primary font-medium"
                              : "text-foreground hover:bg-hover-bg",
                          )}
                        >
                          <span className="truncate">
                            {d.name || d.draft_code}
                          </span>
                          <span className="shrink-0 font-mono-tech text-[10.5px] text-muted-foreground">
                            {d.draft_code}
                          </span>
                        </button>
                      );
                    })}
                  </div>
                </div>
              )}

              {transparentSuppliers.length > 0 && (
                <div className="space-y-0.5 border-t border-border pt-1.5">
                  <div className="px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                    {t("upstreamConnector.publishedSuppliers")}
                  </div>
                  <div className="max-h-40 overflow-y-auto space-y-0.5">
                    {transparentSuppliers.slice(0, 12).map((s) => {
                      const isCurrent = s.id === draft.linked_supplier_id;
                      return (
                        <button
                          key={s.id}
                          type="button"
                          onClick={() => {
                            setSwitcherOpen(false);
                            void switchToSupplier(s.id, navigate);
                          }}
                          className={cn(
                            "flex w-full items-center justify-between gap-2 rounded-md px-2 py-1.5 text-xs text-left cursor-pointer",
                            isCurrent
                              ? "bg-primary-soft text-primary font-medium"
                              : "text-foreground hover:bg-hover-bg",
                          )}
                        >
                          <span className="truncate">{s.name}</span>
                          <span className="shrink-0 font-mono-tech text-[10.5px] text-muted-foreground">
                            {s.supplier_code}
                          </span>
                        </button>
                      );
                    })}
                  </div>
                </div>
              )}
            </Popover>

            {!canWrite && (
              <Badge variant="neutral">
                {t("upstreamConnector.readOnlyHint")}
              </Badge>
            )}

            {saveBadge && (
              <Badge variant={badgeVariant} data-testid="wb-save-badge">
                {saveBadge.text}
              </Badge>
            )}
          </>
        )}
      </div>

      {/* Right: Single unified Env trigger + Controls + Import/Export + Save + Gate + Publish */}
      {draft && (
        <div className="flex shrink-0 items-center gap-1.5">
          {/* Single Global Environment Entry (resolves A-12) */}
          <Popover
            open={envMenuOpen}
            onOpenChange={setEnvMenuOpen}
            align="right"
            trigger={
              <button
                type="button"
                onClick={() => setEnvMenuOpen((v) => !v)}
                title={t("upstreamConnector.namedEnvs")}
                aria-label={t("upstreamConnector.namedEnvs")}
                data-testid="wb-topbar-env-trigger"
                className="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-md border border-border bg-muted/50 px-2.5 text-xs font-medium text-foreground hover:bg-hover-bg hover:border-border-hover transition-colors cursor-pointer"
              >
                <span
                  className={cn(
                    "h-1.5 w-1.5 shrink-0 rounded-full",
                    activeNamedEnv ? "bg-success-fg" : "bg-muted-foreground/60",
                  )}
                />
                <span className="max-w-[110px] truncate">
                  {activeNamedEnv?.name ||
                    t("upstreamConnector.noEnvSelected")}
                </span>
                <span className="rounded-full border border-border bg-card px-1.5 py-0 text-[10.5px] font-mono-tech tabular-nums text-muted-foreground">
                  {envVarCount}
                </span>
                <ChevronDown className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
              </button>
            }
            contentClassName="w-76 p-2 space-y-2"
          >
            <div className="space-y-0.5">
              <button
                type="button"
                onClick={() => {
                  setActiveEnv(null);
                  setEnvMenuOpen(false);
                }}
                className={cn(
                  "flex w-full items-center justify-between rounded-md px-2 py-1.5 text-xs cursor-pointer",
                  !envState.activeEnvId
                    ? "bg-primary-soft text-primary font-medium"
                    : "text-foreground hover:bg-hover-bg",
                )}
              >
                <span>{t("upstreamConnector.noEnvSelected")}</span>
                <span className="font-mono-tech text-[10.5px] tabular-nums text-muted-foreground">
                  {
                    envState.globalRows.filter(
                      (r) => r.enabled !== false && r.key.trim(),
                    ).length
                  }
                </span>
              </button>

              {envState.environments.map((env) => {
                const isCurrent = env.id === envState.activeEnvId;
                const count = env.rows.filter(
                  (r) => r.enabled !== false && r.key.trim(),
                ).length;
                return (
                  <button
                    key={env.id}
                    type="button"
                    onClick={() => {
                      setActiveEnv(env.id);
                      setEnvMenuOpen(false);
                    }}
                    className={cn(
                      "flex w-full items-center justify-between rounded-md px-2 py-1.5 text-xs cursor-pointer",
                      isCurrent
                        ? "bg-primary-soft text-primary font-medium"
                        : "text-foreground hover:bg-hover-bg",
                    )}
                  >
                    <span className="truncate">{env.name}</span>
                    <span className="font-mono-tech text-[10.5px] tabular-nums text-muted-foreground">
                      {count}
                    </span>
                  </button>
                );
              })}
            </div>

            {canWrite && (
              <div className="flex items-center gap-1.5 border-t border-border pt-2">
                <Input
                  type="text"
                  value={newEnvName}
                  onChange={(e) => setNewEnvName(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      handleSubmitNewEnv();
                    }
                  }}
                  placeholder={t("upstreamConnector.envNamePlaceholder")}
                  aria-label={t("upstreamConnector.newEnv")}
                  className="h-7 flex-1 text-xs"
                />
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={handleSubmitNewEnv}
                >
                  {t("upstreamConnector.newEnv")}
                </Button>
              </div>
            )}

            <div className="border-t border-border pt-2 space-y-1">
              <div className="flex items-center justify-between text-[11px] font-medium text-muted-foreground px-1">
                <span>{t("upstreamConnector.envVars")}</span>
                <span className="font-mono-tech tabular-nums">
                  {runtimeEntries.length}
                </span>
              </div>
              {runtimeEntries.length === 0 ? (
                <div className="px-1 py-1.5 text-[11px] text-muted-foreground">
                  {t("upstreamConnector.envPreviewEmpty")}
                </div>
              ) : (
                <div className="max-h-32 overflow-y-auto space-y-1 px-1">
                  {runtimeEntries.map(([k, v]) => (
                    <div
                      key={k}
                      className="flex items-center justify-between gap-2 text-[11px]"
                    >
                      <code className="truncate font-mono-tech text-primary">
                        {k}
                      </code>
                      <span className="truncate font-mono-tech text-muted-foreground">
                        {!canWrite ? (v ? "••••••••" : "∅") : v || "∅"}
                      </span>
                    </div>
                  ))}
                </div>
              )}
            </div>

            <div className="border-t border-border pt-1.5">
              <button
                type="button"
                onClick={() => {
                  setEnvMenuOpen(false);
                  onOpenEnvDrawer();
                }}
                data-testid="wb-open-env-drawer"
                className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-xs font-medium text-primary hover:bg-primary-soft cursor-pointer"
              >
                <SlidersHorizontal className="h-3.5 w-3.5" />
                <span>{t("upstreamConnector.sidebarEnv")}</span>
              </button>
            </div>
          </Popover>

          <IconButton
            size="sm"
            onClick={() => setShowCommands(true)}
            title={`${t("upstreamConnector.commandPalette")} (Ctrl+K)`}
            aria-label={t("upstreamConnector.commandPalette")}
            data-testid="wb-topbar-commands"
          >
            <Command className="h-3.5 w-3.5" />
          </IconButton>

          <IconButton
            size="sm"
            onClick={toggleLocale}
            title={t("config.headerBar.language")}
            aria-label={t("config.headerBar.language")}
            data-testid="wb-topbar-locale"
          >
            <Languages className="h-3.5 w-3.5" />
          </IconButton>

          <IconButton
            size="sm"
            onClick={toggleTheme}
            title={t("config.headerBar.themeToggle")}
            aria-label={t("config.headerBar.themeToggle")}
            data-testid="wb-topbar-theme"
          >
            {isDark ? (
              <Sun className="h-3.5 w-3.5" />
            ) : (
              <Moon className="h-3.5 w-3.5" />
            )}
          </IconButton>

          {canWrite && (
            <>
              <Popover
                open={ioMenuOpen}
                onOpenChange={setIoMenuOpen}
                align="right"
                trigger={
                  <Button
                    type="button"
                    variant="outline"
                    size="md"
                    onClick={() => setIoMenuOpen((v) => !v)}
                    data-testid="wb-topbar-io"
                  >
                    <span>{t("upstreamConnector.importExport")}</span>
                    <ChevronDown className="h-3.5 w-3.5 text-muted-foreground" />
                  </Button>
                }
                contentClassName="w-48 p-1 space-y-0.5"
              >
                <button
                  type="button"
                  onClick={() => {
                    setIoMenuOpen(false);
                    setShowImport(true);
                  }}
                  className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-xs text-foreground hover:bg-hover-bg cursor-pointer"
                >
                  <Upload className="h-3.5 w-3.5 text-muted-foreground" />
                  <span>{t("upstreamConnector.import")}</span>
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setIoMenuOpen(false);
                    void doExport(navigate);
                  }}
                  className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-xs text-foreground hover:bg-hover-bg cursor-pointer"
                >
                  <Download className="h-3.5 w-3.5 text-muted-foreground" />
                  <span>{t("upstreamConnector.exportPostman")}</span>
                </button>
                <div className="my-1 h-px bg-border" />
                <button
                  type="button"
                  onClick={() => {
                    setIoMenuOpen(false);
                    envFileInputRef.current?.click();
                  }}
                  className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-xs text-foreground hover:bg-hover-bg cursor-pointer"
                >
                  <FileJson className="h-3.5 w-3.5 text-muted-foreground" />
                  <span>{t("upstreamConnector.importEnvJson")}</span>
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setIoMenuOpen(false);
                    exportEnvToFile();
                  }}
                  className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-xs text-foreground hover:bg-hover-bg cursor-pointer"
                >
                  <FileJson className="h-3.5 w-3.5 text-muted-foreground" />
                  <span>{t("upstreamConnector.exportEnvJson")}</span>
                </button>
                {draft.linked_supplier_id && (
                  <>
                    <div className="my-1 h-px bg-border" />
                    <button
                      type="button"
                      onClick={() => {
                        setIoMenuOpen(false);
                        void resyncFromLiveSupplier(navigate);
                      }}
                      className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-xs text-foreground hover:bg-hover-bg cursor-pointer"
                    >
                      <RefreshCw className="h-3.5 w-3.5 text-muted-foreground" />
                      <span>{t("upstreamConnector.resyncFromLive")}</span>
                    </button>
                  </>
                )}
              </Popover>

              <input
                ref={envFileInputRef}
                type="file"
                accept=".json,application/json"
                hidden
                onChange={(e) => void handleEnvFilePicked(e)}
              />

              <Button
                type="button"
                variant="outline"
                size="md"
                loading={saving}
                title="Ctrl/⌘+S"
                onClick={() => void saveDraftExplicit(navigate)}
                data-testid="wb-topbar-save"
              >
                {!saving &&
                  (dirty ? (
                    <span className="h-2 w-2 rounded-full bg-warning-fg" />
                  ) : (
                    <Save className="h-3.5 w-3.5 text-muted-foreground" />
                  ))}
                <span>{t("upstreamConnector.saveDraft")}</span>
              </Button>

              <button
                type="button"
                onClick={() => openSidebarTo("gate")}
                title={t("upstreamConnector.gateChecklist")}
                data-testid="wb-topbar-gate"
                className={cn(
                  "inline-flex h-8 shrink-0 items-center gap-1.5 rounded-md border px-2.5 text-xs font-medium tabular-nums transition-colors cursor-pointer",
                  isGateReady
                    ? "border-success-fg/35 bg-success-fg/10 text-success-fg hover:bg-success-fg/15"
                    : "border-border bg-muted/50 text-muted-foreground hover:bg-hover-bg hover:text-foreground",
                )}
              >
                <span
                  className={cn(
                    "h-1.5 w-1.5 rounded-full",
                    isGateReady ? "bg-success-fg" : "bg-warning-fg",
                  )}
                />
                <span>
                  {t("upstreamConnector.gateShort")} {gate.ok}/{gate.total}
                </span>
              </button>

              <Tooltip
                disabled={isGateReady}
                side="bottom"
                content={t("upstreamConnector.gateNotReadyTip", {
                  ok: gate.ok,
                  total: gate.total,
                })}
              >
                <Button
                  type="button"
                  variant="primary"
                  size="md"
                  loading={publishing}
                  onClick={() => void openPublish(navigate)}
                  data-testid="wb-topbar-publish"
                >
                  {!publishing && <UploadCloud className="h-3.5 w-3.5" />}
                  <span>{t("upstreamConnector.publish")}</span>
                </Button>
              </Tooltip>
            </>
          )}
        </div>
      )}
    </header>
  );
}
