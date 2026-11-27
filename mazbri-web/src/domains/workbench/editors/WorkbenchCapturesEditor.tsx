import { useMemo } from "react";
import { Copy, Plus, Trash2, X } from "lucide-react";
import type { CaptureSpec } from "@/api/upstream-connectors";
import { Badge, Button, IconButton } from "@/components/ui";
import { useT } from "@/locales";
import { copyWithFeedback } from "@/utils/ui/copy-feedback";
import { normalizeCaptureRow } from "../engine/workbenchHelpers";

export interface WorkbenchCapturesEditorProps {
  captures: CaptureSpec[];
  capturedVars: Record<string, string>;
  disabled?: boolean;
  onChange: (next: CaptureSpec[]) => void;
  onClearCaptured: () => void;
  onRemoveCaptured: (name: string) => void;
}

export function WorkbenchCapturesEditor({
  captures,
  capturedVars,
  disabled = false,
  onChange,
  onClearCaptured,
  onRemoveCaptured,
}: WorkbenchCapturesEditorProps) {
  const { t } = useT();

  const capturedEntries = useMemo(
    () => Object.entries(capturedVars || {}),
    [capturedVars],
  );

  const addRow = () => {
    onChange([...captures, normalizeCaptureRow({ name: "", path: "" })]);
  };

  const patchRow = (idx: number, patch: Partial<CaptureSpec>) => {
    const next = captures.map((r, i) =>
      i === idx
        ? normalizeCaptureRow({ ...r, ...patch })
        : normalizeCaptureRow(r),
    );
    const last = next[next.length - 1];
    if (last && (last.name.trim() || last.path.trim())) {
      next.push(normalizeCaptureRow({ name: "", path: "" }));
    }
    onChange(next);
  };

  const removeRow = (idx: number) => {
    onChange(captures.filter((_, i) => i !== idx));
  };

  const clearRows = () => {
    onChange([]);
  };

  const copyPlaceholder = (name: string) => {
    const trimmed = name.trim();
    if (!trimmed) return;
    void copyWithFeedback(`{{captures.${trimmed}}}`);
  };

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-1.5">
        <div className="flex items-center justify-between gap-2 min-h-8 px-1">
          <span className="text-xs font-semibold text-muted-foreground">
            {t("upstreamConnector.capturesTab")}
          </span>
          <div className="flex items-center gap-0.5">
            <IconButton
              size="sm"
              disabled={!captures.length || disabled}
              title={t("upstreamConnector.clearKv")}
              aria-label={t("upstreamConnector.clearKv")}
              onClick={clearRows}
            >
              <Trash2 className="h-3.5 w-3.5" />
            </IconButton>
            <IconButton
              size="sm"
              disabled={disabled}
              title={t("upstreamConnector.addCapture")}
              aria-label={t("upstreamConnector.addCapture")}
              onClick={addRow}
            >
              <Plus className="h-3.5 w-3.5" />
            </IconButton>
          </div>
        </div>

        {captures.length > 0 && (
          <div className="w-full overflow-hidden rounded-lg border border-border bg-card">
            {captures.map((row, idx) => {
              const trimmedName = row.name.trim();
              return (
                <div
                  key={row.uid || idx}
                  className="grid grid-cols-[minmax(120px,0.8fr)_minmax(160px,1.2fr)_minmax(140px,0.9fr)_32px] items-stretch border-b border-border-subtle last:border-b-0"
                >
                  <input
                    type="text"
                    value={row.name}
                    disabled={disabled}
                    placeholder={t("upstreamConnector.captureNamePh")}
                    aria-label={t("upstreamConnector.fields.envName")}
                    spellCheck={false}
                    onChange={(e) => patchRow(idx, { name: e.target.value })}
                    className="h-8 min-w-0 border-r border-border-subtle bg-transparent px-2.5 font-mono-tech text-xs text-foreground placeholder:text-muted-foreground outline-none disabled:opacity-50"
                  />
                  <input
                    type="text"
                    value={row.path}
                    disabled={disabled}
                    placeholder={t("upstreamConnector.capturePathPh")}
                    aria-label="JSONPath"
                    spellCheck={false}
                    onChange={(e) => patchRow(idx, { path: e.target.value })}
                    className="h-8 min-w-0 border-r border-border-subtle bg-transparent px-2.5 font-mono-tech text-xs text-foreground placeholder:text-muted-foreground outline-none disabled:opacity-50"
                  />
                  <button
                    type="button"
                    disabled={!trimmedName}
                    title={t("common.actions.copy")}
                    onClick={() => copyPlaceholder(row.name)}
                    className="inline-flex h-8 min-w-0 items-center justify-between gap-1.5 border-r border-border-subtle bg-muted/40 px-2.5 text-left text-muted-foreground transition-colors hover:bg-hover-bg hover:text-foreground disabled:pointer-events-none disabled:opacity-45 cursor-pointer"
                  >
                    <code className="truncate font-mono-tech text-[11px]">
                      {trimmedName
                        ? `{{captures.${trimmedName}}}`
                        : "{{captures.*}}"}
                    </code>
                    <Copy className="h-3 w-3 shrink-0 opacity-70" />
                  </button>
                  <div className="flex items-center justify-center">
                    <button
                      type="button"
                      disabled={disabled}
                      title={t("common.actions.delete")}
                      aria-label={t("common.actions.delete")}
                      onClick={() => removeRow(idx)}
                      className="inline-flex h-6 w-6 items-center justify-center rounded-md text-danger-fg hover:bg-danger-fg/10 disabled:opacity-40 cursor-pointer"
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      <div className="flex flex-col gap-2">
        <div className="flex items-center justify-between gap-2 min-h-8 px-1">
          <div className="inline-flex items-center gap-1.5 text-xs font-semibold text-muted-foreground">
            <span>{t("upstreamConnector.capturedPoolTitle")}</span>
            <Badge variant="neutral" className="font-mono-tech tabular-nums">
              {capturedEntries.length}
            </Badge>
          </div>
          {capturedEntries.length > 0 && (
            <Button
              type="button"
              variant="outline"
              size="xs"
              disabled={disabled}
              onClick={onClearCaptured}
            >
              {t("upstreamConnector.clearKv")}
            </Button>
          )}
        </div>

        {!capturedEntries.length ? (
          <div className="rounded-lg border border-dashed border-border bg-muted/30 px-3 py-4 text-xs text-muted-foreground">
            {t("upstreamConnector.capturedEmpty")}
          </div>
        ) : (
          <div className="w-full overflow-hidden rounded-lg border border-border bg-card">
            {capturedEntries.map(([k, v]) => (
              <div
                key={k}
                className="grid grid-cols-[minmax(160px,0.9fr)_minmax(0,1.5fr)_32px] items-stretch border-b border-border-subtle last:border-b-0"
              >
                <button
                  type="button"
                  title={t("common.actions.copy")}
                  onClick={() => copyPlaceholder(k)}
                  className="inline-flex h-8 min-w-0 items-center justify-between gap-1.5 border-r border-border-subtle bg-muted/40 px-2.5 text-left text-success-fg transition-colors hover:bg-hover-bg cursor-pointer"
                >
                  <code className="truncate font-mono-tech text-[11.5px]">
                    {`{{captures.${k}}}`}
                  </code>
                  <Copy className="h-3 w-3 shrink-0 opacity-75" />
                </button>
                <div className="flex h-8 min-w-0 items-center border-r border-border-subtle px-2.5 font-mono-tech text-xs text-foreground">
                  <span className="truncate">{v}</span>
                </div>
                <div className="flex items-center justify-center">
                  {!disabled && (
                    <button
                      type="button"
                      title={t("common.actions.delete")}
                      aria-label={t("common.actions.delete")}
                      onClick={() => onRemoveCaptured(k)}
                      className="inline-flex h-6 w-6 items-center justify-center rounded-md text-danger-fg hover:bg-danger-fg/10 cursor-pointer"
                    >
                      <X className="h-3.5 w-3.5" />
                    </button>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
