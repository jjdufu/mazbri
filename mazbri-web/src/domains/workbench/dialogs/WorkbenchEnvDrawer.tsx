import React, { useMemo, useRef, useState } from "react";
import {
  Check,
  Copy,
  Download,
  KeyRound,
  Plus,
  Trash2,
  Upload,
} from "lucide-react";
import { useT } from "@/locales";
import { copyWithFeedback } from "@/utils/ui/copy-feedback";
import {
  Badge,
  Button,
  cn,
  CodeEditor,
  Drawer,
  IconButton,
  Input,
  Tooltip,
} from "@/components/ui";
import {
  emptyKvRow,
  kvEnabledCount,
  MASKED_SECRET_VALUE,
  type KvRow,
} from "../engine/workbenchHelpers";
import {
  selectActiveNamedEnv,
  selectCanWriteWorkbench,
  selectSessionCredentialKeys,
  useWorkbenchStore,
} from "../store";

interface EnvKvTableProps {
  rows: KvRow[];
  disabled: boolean;
  maskValues: boolean;
  onChange: (next: KvRow[]) => void;
}

function EnvKvTable({
  rows,
  disabled,
  maskValues,
  onChange,
}: EnvKvTableProps) {
  const { t } = useT();

  const updateRow = (index: number, patch: Partial<KvRow>) => {
    const next = rows.map((row, idx) =>
      idx === index ? { ...row, ...patch } : row,
    );
    onChange(next);
  };

  const removeRow = (index: number) => {
    onChange(rows.filter((_, idx) => idx !== index));
  };

  const addRow = () => {
    onChange([...rows, emptyKvRow()]);
  };

  const clearRows = () => {
    onChange([]);
  };

  return (
    <div className="space-y-2">
      <div className="rounded-lg border border-border overflow-hidden">
        <div className="grid grid-cols-[24px_minmax(0,1fr)_minmax(0,1.2fr)_minmax(0,0.9fr)_28px] items-center gap-1.5 border-b border-border bg-muted/45 px-2 py-1.5 text-[11px] font-medium text-muted-foreground">
          <span />
          <span>{t("upstreamConnector.kvKey")}</span>
          <span>{t("upstreamConnector.kvValue")}</span>
          <span>{t("upstreamConnector.kvDesc")}</span>
          <span />
        </div>

        {rows.length === 0 ? (
          <div className="px-3 py-4 text-center text-xs text-muted-foreground">
            {t("upstreamConnector.envPreviewEmpty")}
          </div>
        ) : (
          <div className="divide-y divide-border-subtle">
            {rows.map((row, idx) => (
              <div
                key={row.uid || idx}
                className={cn(
                  "grid grid-cols-[24px_minmax(0,1fr)_minmax(0,1.2fr)_minmax(0,0.9fr)_28px] items-center gap-1.5 px-2 py-1.5",
                  row.enabled === false && "opacity-55",
                )}
              >
                <div className="flex items-center justify-center">
                  <input
                    type="checkbox"
                    checked={row.enabled !== false}
                    disabled={disabled}
                    aria-label={
                      row.enabled !== false
                        ? t("upstreamConnector.kvDisable")
                        : t("upstreamConnector.kvEnable")
                    }
                    onChange={(e) =>
                      updateRow(idx, { enabled: e.target.checked })
                    }
                  />
                </div>
                <Input
                  value={row.key}
                  disabled={disabled}
                  placeholder={t("upstreamConnector.kvPhKey")}
                  aria-label={t("upstreamConnector.kvKey")}
                  className="h-7 font-mono-tech"
                  onChange={(e) => updateRow(idx, { key: e.target.value })}
                />
                <Input
                  value={maskValues && row.value ? MASKED_SECRET_VALUE : row.value}
                  disabled={disabled}
                  placeholder={t("upstreamConnector.kvPhValue")}
                  aria-label={t("upstreamConnector.kvValue")}
                  className="h-7 font-mono-tech"
                  onChange={(e) => updateRow(idx, { value: e.target.value })}
                />
                <Input
                  value={row.desc}
                  disabled={disabled}
                  placeholder={t("upstreamConnector.kvPhDesc")}
                  aria-label={t("upstreamConnector.kvDesc")}
                  className="h-7"
                  onChange={(e) => updateRow(idx, { desc: e.target.value })}
                />
                <IconButton
                  size="sm"
                  disabled={disabled}
                  aria-label={t("common.actions.delete")}
                  title={t("common.actions.delete")}
                  className="h-6 w-6"
                  onClick={() => removeRow(idx)}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </IconButton>
              </div>
            ))}
          </div>
        )}
      </div>

      {!disabled && (
        <div className="flex items-center justify-between gap-2">
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={addRow}
          >
            <Plus className="h-3.5 w-3.5" />
            {t("upstreamConnector.addEnv")}
          </Button>
          {rows.length > 0 && (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={clearRows}
            >
              {t("upstreamConnector.clearKv")}
            </Button>
          )}
        </div>
      )}
    </div>
  );
}

function WorkbenchEnvDrawerBody() {
  const { t } = useT();
  const state = useWorkbenchStore();
  const envState = state.envState;
  const capturedVars = state.capturedVars;
  const activeNamedEnv = useMemo(() => selectActiveNamedEnv(state), [state]);
  const canWrite = useMemo(() => selectCanWriteWorkbench(state), [state]);
  const credentialKeys = useMemo(
    () => selectSessionCredentialKeys(state),
    [state],
  );

  const [scopeTab, setScopeTab] = useState<"global" | "named">(() =>
    envState.activeEnvId ? "named" : "global",
  );
  const [newEnvName, setNewEnvName] = useState("");
  const [showPasteImport, setShowPasteImport] = useState(false);
  const [pasteJson, setPasteJson] = useState("");
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const capturedEntries = useMemo(
    () => Object.entries(capturedVars || {}),
    [capturedVars],
  );

  const handleCreateEnv = () => {
    const trimmed = newEnvName.trim();
    if (!trimmed) return;
    state.addNamedEnv(trimmed);
    setNewEnvName("");
    setScopeTab("named");
  };

  const handleFileImport = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = "";
    if (!file) return;
    const text = await file.text();
    state.importEnvFromText(text);
  };

  const handlePasteImport = () => {
    const text = pasteJson.trim();
    if (!text) return;
    state.importEnvFromText(text);
    setPasteJson("");
    setShowPasteImport(false);
  };

  const updateGlobalRows = (nextRows: KvRow[]) => {
    state.setEnvState((prev) => ({
      ...prev,
      globalRows: nextRows,
    }));
  };

  const updateNamedEnvRows = (envId: string, nextRows: KvRow[]) => {
    state.setEnvState((prev) => ({
      ...prev,
      environments: prev.environments.map((env) =>
        env.id === envId ? { ...env, rows: nextRows } : env,
      ),
    }));
  };

  const renameNamedEnv = (envId: string, name: string) => {
    state.setEnvState((prev) => ({
      ...prev,
      environments: prev.environments.map((env) =>
        env.id === envId ? { ...env, name } : env,
      ),
    }));
  };

  return (
    <Drawer
      open
      onClose={() => state.setEnvDrawerOpen(false)}
      title={t("upstreamConnector.envVars")}
      size="lg"
      footer={
        <>
          <input
            ref={fileInputRef}
            type="file"
            accept=".json,application/json"
            hidden
            onChange={(e) => void handleFileImport(e)}
          />
          {canWrite && (
            <Button
              type="button"
              variant="outline"
              onClick={() => setShowPasteImport((v) => !v)}
            >
              <Upload className="h-3.5 w-3.5" />
              {t("upstreamConnector.importEnvJson")}
            </Button>
          )}
          <Button
            type="button"
            variant="outline"
            onClick={() => state.exportEnvToFile()}
          >
            <Download className="h-3.5 w-3.5" />
            {t("upstreamConnector.exportEnvJson")}
          </Button>
          <Button
            type="button"
            variant="primary"
            onClick={() => state.setEnvDrawerOpen(false)}
          >
            {t("common.actions.close")}
          </Button>
        </>
      }
    >
      <div className="space-y-5">
        {showPasteImport && canWrite && (
          <section className="rounded-lg border border-border bg-muted/30 p-3 space-y-2.5">
            <div className="flex items-center justify-between gap-2">
              <span className="text-xs font-semibold text-foreground">
                {t("upstreamConnector.importEnvJson")}
              </span>
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => fileInputRef.current?.click()}
              >
                <Upload className="h-3.5 w-3.5" />
                {t("upstreamConnector.importFile")}
              </Button>
            </div>
            <CodeEditor
              value={pasteJson}
              onChange={setPasteJson}
              language="json"
              minHeight={110}
              maxHeight={200}
              placeholder='{"BASE_HOST": "https://api.example.com"}'
            />
            <div className="flex items-center justify-end gap-2">
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => {
                  setShowPasteImport(false);
                  setPasteJson("");
                }}
              >
                {t("common.cancel")}
              </Button>
              <Button
                type="button"
                variant="primary"
                size="sm"
                disabled={!pasteJson.trim()}
                onClick={handlePasteImport}
              >
                {t("common.confirm")}
              </Button>
            </div>
          </section>
        )}

        {/* Environment selector & create */}
        <section className="space-y-2.5">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <span className="text-xs font-semibold text-foreground">
              {t("upstreamConnector.namedEnvs")}
            </span>
            {canWrite && (
              <div className="flex items-center gap-1.5">
                <Input
                  value={newEnvName}
                  onChange={(e) => setNewEnvName(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      handleCreateEnv();
                    }
                  }}
                  placeholder={t("upstreamConnector.envNamePlaceholder")}
                  aria-label={t("upstreamConnector.newEnv")}
                  className="h-7 w-40"
                />
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={!newEnvName.trim()}
                  onClick={handleCreateEnv}
                >
                  <Plus className="h-3.5 w-3.5" />
                  {t("upstreamConnector.newEnv")}
                </Button>
              </div>
            )}
          </div>

          <div className="flex flex-wrap items-center gap-1.5">
            <button
              type="button"
              onClick={() => {
                state.setActiveEnv(null);
                setScopeTab("global");
              }}
              className={cn(
                "inline-flex h-7 items-center gap-1.5 rounded-md border px-2.5 text-xs font-medium transition-colors cursor-pointer",
                !envState.activeEnvId && scopeTab === "global"
                  ? "border-primary/60 bg-primary-soft text-primary"
                  : !envState.activeEnvId
                    ? "border-primary/40 bg-primary-soft/50 text-primary"
                    : "border-border bg-card text-foreground hover:bg-hover-bg",
              )}
            >
              {!envState.activeEnvId && <Check className="h-3 w-3" />}
              <span>{t("upstreamConnector.globalEnv")}</span>
              <Badge variant="neutral" className="px-1.5 py-0 text-[10px]">
                {kvEnabledCount(envState.globalRows)}
              </Badge>
            </button>

            {envState.environments.map((env) => {
              const isActive = env.id === envState.activeEnvId;
              return (
                <button
                  key={env.id}
                  type="button"
                  onClick={() => {
                    state.setActiveEnv(env.id);
                    setScopeTab("named");
                  }}
                  className={cn(
                    "inline-flex h-7 items-center gap-1.5 rounded-md border px-2.5 text-xs font-medium transition-colors cursor-pointer",
                    isActive
                      ? "border-primary/60 bg-primary-soft text-primary"
                      : "border-border bg-card text-foreground hover:bg-hover-bg",
                  )}
                >
                  {isActive && <Check className="h-3 w-3" />}
                  <span>{env.name}</span>
                  <Badge variant="neutral" className="px-1.5 py-0 text-[10px]">
                    {kvEnabledCount(env.rows)}
                  </Badge>
                </button>
              );
            })}
          </div>
        </section>

        {/* Active Named Environment Editor */}
        {activeNamedEnv && (
          <section className="rounded-lg border border-border bg-muted/20 p-3 space-y-3">
            <div className="flex items-center justify-between gap-2">
              <div className="flex items-center gap-2 min-w-0 flex-1">
                <Input
                  value={activeNamedEnv.name}
                  disabled={!canWrite}
                  aria-label={t("upstreamConnector.namedEnvs")}
                  className="h-7 max-w-[220px] font-medium"
                  onChange={(e) =>
                    renameNamedEnv(activeNamedEnv.id, e.target.value)
                  }
                />
                <Badge variant="xblue">{t("upstreamConnector.namedEnvs")}</Badge>
              </div>
              {canWrite && (
                <Tooltip content={t("upstreamConnector.deleteEnv")} side="left">
                  <Button
                    type="button"
                    variant="danger"
                    size="sm"
                    onClick={() => state.deleteNamedEnv(activeNamedEnv.id)}
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                    {t("upstreamConnector.deleteEnv")}
                  </Button>
                </Tooltip>
              )}
            </div>

            <EnvKvTable
              rows={activeNamedEnv.rows}
              disabled={!canWrite}
              maskValues={!canWrite}
              onChange={(nextRows) =>
                updateNamedEnvRows(activeNamedEnv.id, nextRows)
              }
            />
          </section>
        )}

        {/* Global Variables Editor */}
        <section className="space-y-2.5">
          <div className="flex items-center justify-between gap-2">
            <span className="text-xs font-semibold text-foreground">
              {t("upstreamConnector.globalEnv")}
            </span>
            <Badge variant="neutral" className="font-mono-tech tabular-nums">
              {kvEnabledCount(envState.globalRows)}
            </Badge>
          </div>
          <EnvKvTable
            rows={envState.globalRows}
            disabled={!canWrite}
            maskValues={!canWrite}
            onChange={updateGlobalRows}
          />
        </section>

        {/* Captured Runtime Variables */}
        <section className="space-y-2">
          <div className="flex items-center justify-between gap-2">
            <span className="text-xs font-semibold text-foreground">
              {t("upstreamConnector.capturedPoolTitle")}
            </span>
            <div className="flex items-center gap-2">
              <Badge variant="neutral" className="font-mono-tech tabular-nums">
                {capturedEntries.length}
              </Badge>
              {capturedEntries.length > 0 && canWrite && (
                <Button
                  type="button"
                  variant="ghost"
                  size="xs"
                  onClick={() => state.clearCapturedVars()}
                >
                  {t("upstreamConnector.clearKv")}
                </Button>
              )}
            </div>
          </div>

          {capturedEntries.length === 0 ? (
            <div className="rounded-lg border border-dashed border-border px-3 py-3 text-center text-xs text-muted-foreground">
              {t("upstreamConnector.capturedEmpty")}
            </div>
          ) : (
            <div className="rounded-lg border border-border overflow-hidden divide-y divide-border-subtle">
              {capturedEntries.map(([k, v]) => (
                <div
                  key={k}
                  className="grid grid-cols-[minmax(140px,0.8fr)_minmax(0,1.4fr)_auto] items-center gap-2 px-3 py-1.5 text-xs font-mono-tech"
                >
                  <button
                    type="button"
                    onClick={() => void copyWithFeedback(`{{captures.${k}}}`)}
                    title={t("common.actions.copy")}
                    className="inline-flex items-center gap-1 text-left text-success-fg hover:underline cursor-pointer truncate"
                  >
                    <span className="truncate">{`{{captures.${k}}}`}</span>
                    <Copy className="h-3 w-3 shrink-0" />
                  </button>
                  <span className="truncate text-foreground" title={v}>
                    {v}
                  </span>
                  {canWrite && (
                    <IconButton
                      size="sm"
                      aria-label={t("common.actions.delete")}
                      className="h-6 w-6"
                      onClick={() => state.removeCapturedVar(k)}
                    >
                      <Trash2 className="h-3 w-3" />
                    </IconButton>
                  )}
                </div>
              ))}
            </div>
          )}
        </section>

        {/* Session Key Vault shortcut */}
        <section className="flex items-center justify-between gap-3 rounded-lg border border-border bg-muted/35 px-3 py-2.5">
          <div className="flex items-center gap-2 min-w-0">
            <KeyRound className="h-4 w-4 text-muted-foreground shrink-0" />
            <span className="text-xs font-medium text-foreground truncate">
              {t("upstreamConnector.authVaultSection")}
            </span>
            <Badge variant="neutral" className="font-mono-tech tabular-nums">
              {credentialKeys.length}
            </Badge>
          </div>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => {
              state.setEnvDrawerOpen(false);
              state.setReqTab("auth");
            }}
          >
            {t("upstreamConnector.authTab")}
          </Button>
        </section>
      </div>
    </Drawer>
  );
}

export function WorkbenchEnvDrawer() {
  const open = useWorkbenchStore((s) => s.envDrawerOpen);
  if (!open) return null;
  return <WorkbenchEnvDrawerBody />;
}
