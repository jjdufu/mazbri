import React, { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import {
  Check,
  ChevronDown,
  ChevronRight,
  Edit3,
  GripVertical,
  HelpCircle,
  Plus,
  Trash2,
} from "lucide-react";
import { IconButton, Popover, Textarea, cn } from "@/components/ui";
import { useT } from "@/locales";
import {
  duplicateKvKeys,
  emptyKvRow,
  isKvBlank,
  normalizeKvRow,
  parseKvBulk,
  serializeKvBulk,
  type KvRow,
} from "../engine/workbenchHelpers";
import { COMMON_HEADERS } from "../engine/hopp/commonHeaders";
import { WorkbenchEnvText } from "./WorkbenchEnvText";

export interface WorkbenchKvRowsProps {
  rows: Array<Partial<KvRow> & { key?: string; value?: string }>;
  onChange: (next: KvRow[]) => void;
  title: string;
  addTitle?: string;
  clearTitle?: string;
  helpTitle?: string;
  keyPlaceholder?: string;
  valuePlaceholder?: string;
  descPlaceholder?: string;
  env?: Record<string, string>;
  variant?: "default" | "headers" | "keys";
  secretValues?: boolean;
  requiredKeys?: string[];
  emptyCompact?: boolean;
  foldable?: boolean;
  disabled?: boolean;
  maskPreview?: boolean;
}

function HeaderKeyAutoComplete({
  value,
  onChange,
  placeholder,
  ariaLabel,
  disabled,
}: {
  value: string;
  onChange: (next: string) => void;
  placeholder: string;
  ariaLabel: string;
  disabled?: boolean;
}) {
  const inputRef = useRef<HTMLInputElement | null>(null);
  const [open, setOpen] = useState(false);
  const [activeIdx, setActiveIdx] = useState(0);
  const [pos, setPos] = useState<{ top: number; left: number; width: number }>({
    top: 0,
    left: 0,
    width: 220,
  });

  const matches = useMemo(() => {
    const q = value.trim().toLowerCase();
    const list = q
      ? COMMON_HEADERS.filter((h) => h.toLowerCase().includes(q))
      : COMMON_HEADERS;
    return list.slice(0, 12);
  }, [value]);

  const measure = () => {
    const el = inputRef.current;
    if (!el || typeof window === "undefined") return;
    const rect = el.getBoundingClientRect();
    setPos({
      top: Math.round(rect.bottom + 4),
      left: Math.round(rect.left),
      width: Math.max(200, Math.round(rect.width)),
    });
  };

  const commit = (next: string) => {
    onChange(next);
    setOpen(false);
  };

  return (
    <>
      <input
        ref={inputRef}
        type="text"
        value={value}
        placeholder={placeholder}
        aria-label={ariaLabel}
        disabled={disabled}
        spellCheck={false}
        autoComplete="off"
        onFocus={() => {
          measure();
          setOpen(true);
          setActiveIdx(0);
        }}
        onBlur={() => setOpen(false)}
        onChange={(e) => {
          onChange(e.target.value);
          measure();
          setOpen(true);
          setActiveIdx(0);
        }}
        onKeyDown={(e) => {
          if (!open || matches.length === 0) return;
          if (e.key === "ArrowDown") {
            e.preventDefault();
            setActiveIdx((i) => (i + 1) % matches.length);
          } else if (e.key === "ArrowUp") {
            e.preventDefault();
            setActiveIdx((i) => (i - 1 + matches.length) % matches.length);
          } else if (e.key === "Enter" || e.key === "Tab") {
            const pick = matches[activeIdx];
            if (pick && pick.toLowerCase() !== value.trim().toLowerCase()) {
              e.preventDefault();
              commit(pick);
            }
          } else if (e.key === "Escape") {
            setOpen(false);
          }
        }}
        className="h-8 w-full min-w-0 bg-transparent px-2.5 text-xs text-foreground placeholder:text-muted-foreground outline-none disabled:cursor-not-allowed disabled:opacity-50"
      />
      {open &&
        !disabled &&
        matches.length > 0 &&
        typeof document !== "undefined" &&
        createPortal(
          <div
            role="listbox"
            style={{
              position: "fixed",
              top: `${pos.top}px`,
              left: `${pos.left}px`,
              width: `${pos.width}px`,
              zIndex: 90,
            }}
            className="max-h-52 overflow-y-auto rounded-lg border border-border bg-card p-1 shadow-lg"
          >
            {matches.map((item, idx) => (
              <button
                key={item}
                type="button"
                role="option"
                aria-selected={idx === activeIdx}
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => commit(item)}
                className={cn(
                  "flex w-full items-center rounded-md px-2 py-1.5 text-left font-mono-tech text-xs transition-colors cursor-pointer",
                  idx === activeIdx
                    ? "bg-primary-soft text-primary font-medium"
                    : "text-foreground hover:bg-hover-bg",
                )}
              >
                {item}
              </button>
            ))}
          </div>,
          document.body,
        )}
    </>
  );
}

export function WorkbenchKvRows({
  rows: rawRows,
  onChange,
  title,
  addTitle = "",
  clearTitle = "",
  helpTitle = "",
  keyPlaceholder = "",
  valuePlaceholder = "",
  descPlaceholder = "",
  env = {},
  variant = "default",
  secretValues = false,
  requiredKeys = [],
  emptyCompact = false,
  foldable = false,
  disabled = false,
  maskPreview = false,
}: WorkbenchKvRowsProps) {
  const { t } = useT();
  const [bulkOpen, setBulkOpen] = useState(false);
  const [bulkText, setBulkText] = useState("");
  const [helpOpen, setHelpOpen] = useState(false);
  const [forceShow, setForceShow] = useState(false);
  const [userFolded, setUserFolded] = useState<boolean | null>(null);
  const [dragIdx, setDragIdx] = useState<number | null>(null);

  const isKeys = variant === "keys";
  const isHeaderKey = variant === "headers" || variant === "keys";

  const keyPh = keyPlaceholder || t("upstreamConnector.kvPhKey");
  const valuePh = valuePlaceholder || t("upstreamConnector.kvPhValue");
  const descPh = descPlaceholder || t("upstreamConnector.kvPhDesc");

  const rows = useMemo(
    () => (Array.isArray(rawRows) ? rawRows.map(normalizeKvRow) : []),
    [rawRows],
  );

  const filledCount = useMemo(
    () => rows.filter((r) => !isKvBlank(r)).length,
    [rows],
  );
  const hasData = filledCount > 0;

  const folded = useMemo(() => {
    if (!foldable) return false;
    if (userFolded !== null) return userFolded;
    return !hasData;
  }, [foldable, userFolded, hasData]);

  const foldSummary = hasData
    ? t("upstreamConnector.sectionItemCount", { n: filledCount })
    : t("upstreamConnector.sectionUnconfigured");

  const canClear = rows.length > 0;
  const dupKeys = useMemo(() => duplicateKvKeys(rows), [rows]);
  const helpText = helpTitle || t("upstreamConnector.kvHelpDetail");

  const tableVisible = useMemo(() => {
    if (rows.length === 0) return false;
    if (!emptyCompact) return true;
    if (forceShow) return true;
    return hasData;
  }, [rows.length, emptyCompact, forceShow, hasData]);

  useEffect(() => {
    if (secretValues && bulkOpen) {
      setBulkOpen(false);
      setBulkText("");
    }
  }, [secretValues, bulkOpen]);

  const emitRows = (next: KvRow[]) => {
    onChange(next.map(normalizeKvRow));
  };

  const afterEdit = (next: KvRow[]) => {
    let edited = next.map(normalizeKvRow);
    if (edited.length === 0) {
      emitRows([]);
      return;
    }
    const last = edited[edited.length - 1]!;
    if (!isKvBlank(last)) {
      edited = [...edited, emptyKvRow()];
    } else {
      while (
        edited.length > 1 &&
        isKvBlank(edited[edited.length - 1]!) &&
        isKvBlank(edited[edited.length - 2]!)
      ) {
        edited = edited.slice(0, -1);
      }
    }
    emitRows(edited);
  };

  const patchRow = (idx: number, patch: Partial<KvRow>) => {
    const base = rows.map((r) => ({ ...r }));
    if (idx < 0 || idx >= base.length) return;
    base[idx] = normalizeKvRow({ ...base[idx], ...patch });
    afterEdit(base);
  };

  const add = () => {
    setBulkOpen(false);
    setForceShow(true);
    if (foldable) setUserFolded(false);
    if (emptyCompact && (rows.length === 0 || rows.every(isKvBlank))) {
      emitRows([emptyKvRow()]);
      return;
    }
    emitRows([...rows, emptyKvRow()]);
  };

  const remove = (idx: number) => {
    emitRows(rows.filter((_, i) => i !== idx));
  };

  const clear = () => {
    if (!canClear) return;
    setForceShow(false);
    emitRows([]);
  };

  const toggleEnabled = (idx: number) => {
    emitRows(
      rows.map((row, i) =>
        i === idx
          ? normalizeKvRow({ ...row, enabled: row.enabled === false })
          : row,
      ),
    );
  };

  const toggleBulk = () => {
    if (secretValues) return;
    if (foldable) setUserFolded(false);
    if (bulkOpen) {
      emitRows(parseKvBulk(bulkText));
      setBulkOpen(false);
      return;
    }
    setBulkText(serializeKvBulk(rows));
    setBulkOpen(true);
  };

  const applyBulk = () => {
    emitRows(parseKvBulk(bulkText));
  };

  const isRowRequiredInvalid = (row: KvRow): boolean => {
    const key = row.key.trim();
    if (!key || !requiredKeys.includes(key)) return false;
    return !row.value.trim();
  };

  const handleDrop = (targetIdx: number) => {
    if (dragIdx === null || dragIdx === targetIdx) {
      setDragIdx(null);
      return;
    }
    if (targetIdx === rows.length - 1 && isKvBlank(rows[targetIdx]!)) {
      setDragIdx(null);
      return;
    }
    const next = [...rows];
    const [moved] = next.splice(dragIdx, 1);
    if (moved) {
      next.splice(targetIdx, 0, moved);
      emitRows(next);
    }
    setDragIdx(null);
  };

  return (
    <div className="flex flex-col min-h-0">
      <div
        onClick={() => {
          if (foldable) setUserFolded(!folded);
        }}
        className={cn(
          "flex items-center justify-between gap-2 min-h-8 px-1 pb-1.5 shrink-0",
          foldable && "cursor-pointer select-none",
          folded && "pb-0",
        )}
      >
        <div className="flex items-center gap-1.5 min-w-0">
          {foldable &&
            (folded ? (
              <ChevronRight className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
            ) : (
              <ChevronDown className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
            ))}
          <span className="text-xs font-medium text-muted-foreground truncate">
            {title}
          </span>
          {foldable && folded && (
            <span className="text-[11px] text-muted-foreground/80 whitespace-nowrap">
              {foldSummary}
            </span>
          )}
        </div>

        <div
          className="flex items-center gap-0.5 shrink-0"
          onClick={(e) => e.stopPropagation()}
        >
          <Popover
            open={helpOpen}
            onOpenChange={setHelpOpen}
            align="right"
            contentClassName="w-72 p-2.5"
            trigger={
              <IconButton
                size="sm"
                title={helpTitle || t("upstreamConnector.kvHelp")}
                aria-label={helpTitle || t("upstreamConnector.kvHelp")}
                onClick={() => setHelpOpen((v) => !v)}
              >
                <HelpCircle className="h-3.5 w-3.5" />
              </IconButton>
            }
          >
            <p className="text-xs leading-relaxed text-foreground">
              {helpText}
            </p>
          </Popover>

          <IconButton
            size="sm"
            disabled={!canClear || disabled}
            title={clearTitle || t("upstreamConnector.clearKv")}
            aria-label={clearTitle || t("upstreamConnector.clearKv")}
            onClick={clear}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </IconButton>

          {!secretValues && (
            <IconButton
              size="sm"
              active={bulkOpen}
              disabled={disabled}
              title={t("upstreamConnector.kvBulk")}
              aria-label={t("upstreamConnector.kvBulk")}
              onClick={toggleBulk}
            >
              <Edit3 className="h-3.5 w-3.5" />
            </IconButton>
          )}

          <IconButton
            size="sm"
            disabled={disabled}
            title={addTitle || title}
            aria-label={addTitle || title}
            onClick={add}
          >
            <Plus className="h-3.5 w-3.5" />
          </IconButton>
        </div>
      </div>

      {!secretValues && !folded && bulkOpen ? (
        <Textarea
          value={bulkText}
          onChange={(e) => setBulkText(e.target.value)}
          onBlur={applyBulk}
          disabled={disabled}
          rows={7}
          aria-label={t("upstreamConnector.kvBulk")}
          placeholder={t("upstreamConnector.kvBulkHint")}
          className="font-mono-tech text-xs leading-relaxed"
        />
      ) : !folded && tableVisible ? (
        <div className="w-full overflow-hidden rounded-lg border border-border bg-card">
          {rows.map((row, idx) => {
            const isTail = idx === rows.length - 1;
            const isOff = row.enabled === false;
            const isInvalid = isRowRequiredInvalid(row);
            return (
              <div
                key={row.uid || idx}
                draggable={!disabled && !isTail}
                onDragStart={() => setDragIdx(idx)}
                onDragOver={(e) => {
                  if (!disabled && !isTail) e.preventDefault();
                }}
                onDrop={() => handleDrop(idx)}
                onDragEnd={() => setDragIdx(null)}
                className={cn(
                  "group grid items-stretch border-b border-border-subtle last:border-b-0 transition-colors",
                  isKeys
                    ? "grid-cols-[28px_minmax(0,1fr)_32px_32px]"
                    : "grid-cols-[28px_minmax(0,1fr)_minmax(0,1fr)_minmax(0,1fr)_32px_32px]",
                  isInvalid && "bg-danger-fg/6 border-b-danger-fg/40",
                  dragIdx === idx && "bg-hover-bg opacity-70",
                )}
              >
                <div className="flex items-center justify-center border-r border-border-subtle">
                  {!isTail && !disabled && (
                    <span
                      title={t("upstreamConnector.kvDrag")}
                      className="inline-flex h-6 w-6 items-center justify-center text-muted-foreground opacity-0 group-hover:opacity-100 cursor-grab active:cursor-grabbing"
                    >
                      <GripVertical className="h-3.5 w-3.5" />
                    </span>
                  )}
                </div>

                <div
                  className={cn(
                    "min-w-0 border-r border-border-subtle",
                    isOff && "opacity-50",
                  )}
                >
                  {isHeaderKey ? (
                    <HeaderKeyAutoComplete
                      value={row.key}
                      onChange={(next) => patchRow(idx, { key: next })}
                      placeholder={keyPh}
                      ariaLabel={`${title} ${keyPh}`}
                      disabled={disabled}
                    />
                  ) : (
                    <input
                      type="text"
                      value={row.key}
                      placeholder={keyPh}
                      aria-label={`${title} ${keyPh}`}
                      disabled={disabled}
                      spellCheck={false}
                      autoComplete="off"
                      onChange={(e) => patchRow(idx, { key: e.target.value })}
                      className="h-8 w-full min-w-0 bg-transparent px-2.5 text-xs text-foreground placeholder:text-muted-foreground outline-none disabled:cursor-not-allowed disabled:opacity-50"
                    />
                  )}
                </div>

                {!isKeys && (
                  <>
                    <div
                      className={cn(
                        "min-w-0 border-r border-border-subtle",
                        isOff && "opacity-50",
                      )}
                    >
                      <WorkbenchEnvText
                        value={row.value}
                        onChange={(next) => patchRow(idx, { value: next })}
                        env={env}
                        placeholder={valuePh}
                        ariaLabel={`${title} ${valuePh}`}
                        disabled={disabled}
                        secret={secretValues}
                        maskPreview={maskPreview}
                      />
                    </div>
                    <div
                      className={cn(
                        "min-w-0 border-r border-border-subtle",
                        isOff && "opacity-50",
                      )}
                    >
                      <input
                        type="text"
                        value={row.desc}
                        placeholder={descPh}
                        aria-label={`${title} ${descPh}`}
                        disabled={disabled}
                        spellCheck={false}
                        autoComplete="off"
                        onChange={(e) =>
                          patchRow(idx, { desc: e.target.value })
                        }
                        className="h-8 w-full min-w-0 bg-transparent px-2.5 text-xs text-foreground placeholder:text-muted-foreground outline-none disabled:cursor-not-allowed disabled:opacity-50"
                      />
                    </div>
                  </>
                )}

                <div className="flex items-center justify-center border-r border-border-subtle">
                  <button
                    type="button"
                    disabled={disabled}
                    onClick={() => toggleEnabled(idx)}
                    title={
                      isOff
                        ? t("upstreamConnector.kvEnable")
                        : t("upstreamConnector.kvDisable")
                    }
                    aria-label={
                      isOff
                        ? t("upstreamConnector.kvEnable")
                        : t("upstreamConnector.kvDisable")
                    }
                    className={cn(
                      "inline-flex h-6 w-6 items-center justify-center rounded-md transition-colors hover:bg-hover-bg disabled:opacity-40 cursor-pointer",
                      isOff ? "text-muted-foreground/50" : "text-success-fg",
                    )}
                  >
                    <Check className="h-3.5 w-3.5" />
                  </button>
                </div>

                <div className="flex items-center justify-center">
                  <button
                    type="button"
                    disabled={disabled}
                    onClick={() => remove(idx)}
                    title={t("common.actions.delete")}
                    aria-label={t("common.actions.delete")}
                    className="inline-flex h-6 w-6 items-center justify-center rounded-md text-danger-fg transition-colors hover:bg-danger-fg/10 disabled:opacity-40 cursor-pointer"
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      ) : null}

      {!folded && dupKeys.length > 0 && (
        <p className="mt-1.5 px-1 text-xs text-warning-fg">
          {t("upstreamConnector.kvDupWarn", { keys: dupKeys.join(", ") })}
        </p>
      )}
    </div>
  );
}
