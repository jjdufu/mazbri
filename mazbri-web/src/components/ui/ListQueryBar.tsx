import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import {
  Code2,
  Filter,
  Info,
  ListChecks,
  ListFilter as ListFilterIcon,
  Search,
  X,
} from "lucide-react";
import { useT } from "@/locales";
import type {
  FilterFieldDef,
  FilterOp,
  ListFilter,
  ResourceFilterCatalog,
} from "@/types/list-query";
import {
  catalogFilterableFields,
  maxFilters,
} from "@/utils/list-query/catalog";
import {
  buildAliasMaps,
  matchFieldRefAtStart,
} from "@/utils/list-query/field-aliases";
import { mergeByFieldOp } from "@/utils/list-query/merge-filters";
import {
  looksLikeStrongClause,
  parseClause,
} from "@/utils/list-query/parse-clause";
import { formatFilterChipText } from "@/utils/list-query/serialize";
import {
  buildSuggestItems,
  type SuggestItem,
  type SuggestKind,
} from "@/utils/list-query/suggest";
import { showWarningToast } from "@/utils/ui/message-guard";
import { cn } from "./cn";
import { Popover } from "./primitives";

const LIVE_COMMIT_MS = 380;

export interface StructuredFilterBarProps {
  q: string;
  onQChange: (next: string) => void;
  filters: readonly ListFilter[];
  onFiltersChange: (next: ListFilter[]) => void;
  catalog: ResourceFilterCatalog;
  placeholder?: string;
  onSearch: () => void;
  className?: string;
  liveCommit?: boolean;
}

interface CommittedChipItem {
  id: string;
  text: string;
  title: string;
  onRemove: () => void;
}

function SuggestKindIcon({ kind }: { kind: SuggestKind }) {
  switch (kind) {
    case "field":
      return <ListFilterIcon className="h-3.5 w-3.5" />;
    case "op":
      return <Code2 className="h-3.5 w-3.5" />;
    case "enum":
      return <ListChecks className="h-3.5 w-3.5" />;
    case "hint":
      return <Info className="h-3.5 w-3.5" />;
    default:
      return <Filter className="h-3.5 w-3.5" />;
  }
}

export function StructuredFilterBar({
  q,
  onQChange,
  filters,
  onFiltersChange,
  catalog,
  placeholder,
  onSearch,
  className,
  liveCommit = true,
}: StructuredFilterBarProps) {
  const { t } = useT();

  const [draftText, setDraftText] = useState("");
  const [suggestOpen, setSuggestOpen] = useState(false);
  const [suggestIndex, setSuggestIndex] = useState(-1);
  const [parseError, setParseError] = useState<string | null>(null);
  const [overflowOpen, setOverflowOpen] = useState(false);
  const [suggestCoords, setSuggestCoords] = useState<{
    top: number;
    left: number;
    width: number;
  } | null>(null);

  const rootRef = useRef<HTMLDivElement | null>(null);
  const shellRef = useRef<HTMLDivElement | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const suggestRef = useRef<HTMLUListElement | null>(null);
  const composingRef = useRef(false);
  const liveCommitTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );
  const pendingSearchRef = useRef(false);

  const qRef = useRef(q);
  const filtersRef = useRef(filters);
  const draftRef = useRef(draftText);
  qRef.current = q;
  filtersRef.current = filters;
  draftRef.current = draftText;

  // Fire onSearch after React commits updated q/filters to avoid stale closures in callers
  useEffect(() => {
    if (!pendingSearchRef.current) return;
    pendingSearchRef.current = false;
    onSearch();
  }, [q, filters, onSearch]);

  const scheduleParentSearch = useCallback(() => {
    pendingSearchRef.current = true;
  }, []);

  const clearLiveCommitTimer = useCallback(() => {
    if (liveCommitTimerRef.current !== null) {
      clearTimeout(liveCommitTimerRef.current);
      liveCommitTimerRef.current = null;
    }
  }, []);

  useEffect(() => clearLiveCommitTimer, [clearLiveCommitTimer]);

  const translateKey = useCallback((key: string) => t(key), [t]);

  const aliasMaps = useMemo(
    () => buildAliasMaps(catalog, translateKey),
    [catalog, translateKey],
  );

  const filterableFields = useMemo(
    () => catalogFilterableFields(catalog),
    [catalog],
  );

  const fieldMap = useMemo(() => {
    const map = new Map<string, FilterFieldDef>();
    for (const f of filterableFields) {
      map.set(f.field, f);
    }
    return map;
  }, [filterableFields]);

  const resolveFieldLabel = useCallback(
    (def: FilterFieldDef) => {
      const label = t(def.label_key);
      return label === def.label_key ? def.field : label;
    },
    [t],
  );

  const resolveOpLabel = useCallback(
    (op: FilterOp) => {
      const key = `listFilters.ops.${op}`;
      const label = t(key);
      return label === key ? op : label;
    },
    [t],
  );

  const resolveOpDetail = useCallback(
    (op: FilterOp) => {
      const key = `listQueryBar.opDetail.${op}`;
      const label = t(key);
      return label === key ? undefined : label;
    },
    [t],
  );

  const defaultCatalogPlaceholder = useMemo(() => {
    const qFields = catalog.fields.filter((f) => f.in_q && !f.ui_hidden);
    if (!qFields.length) {
      return t("listQueryBar.placeholderQOnly");
    }
    const picked: string[] = [];
    let charBudget = 0;
    for (const field of qFields) {
      const label = t(field.label_key).trim();
      if (!label || picked.includes(label)) continue;
      const nextBudget =
        charBudget + (picked.length > 0 ? 3 : 0) + label.length;
      if (picked.length > 0 && nextBudget > 14) {
        break;
      }
      picked.push(label);
      charBudget = nextBudget;
      if (picked.length >= 2) break;
    }
    return picked.length > 0
      ? picked.join(" / ")
      : t("listQueryBar.placeholderQOnly");
  }, [catalog.fields, t]);

  const removeFilterAt = useCallback(
    (index: number) => {
      const cur = filtersRef.current;
      if (index < 0 || index >= cur.length) return;
      const next = cur.slice();
      next.splice(index, 1);
      scheduleParentSearch();
      onFiltersChange(next);
    },
    [onFiltersChange, scheduleParentSearch],
  );

  const clearQ = useCallback(() => {
    if (!qRef.current.trim()) return;
    scheduleParentSearch();
    onQChange("");
  }, [onQChange, scheduleParentSearch]);

  const clearAllQuery = useCallback(() => {
    clearLiveCommitTimer();
    const hadQ = Boolean(qRef.current.trim());
    const hadFilters = filtersRef.current.length > 0;
    setDraftText("");
    setParseError(null);
    setOverflowOpen(false);
    setSuggestOpen(true);
    setSuggestIndex(-1);
    if (hadQ || hadFilters) {
      scheduleParentSearch();
      if (hadQ) onQChange("");
      if (hadFilters) onFiltersChange([]);
    } else {
      onSearch();
    }
    requestAnimationFrame(() => inputRef.current?.focus());
  }, [
    clearLiveCommitTimer,
    onFiltersChange,
    onQChange,
    onSearch,
    scheduleParentSearch,
  ]);

  const committedChipItems = useMemo<CommittedChipItem[]>(() => {
    const items: CommittedChipItem[] = filters.map((f, index) => {
      const def = fieldMap.get(f.field);
      const fieldLabel = def ? resolveFieldLabel(def) : f.field;
      const text = formatFilterChipText({
        fieldLabel,
        opLabel: resolveOpLabel(f.op),
        value: f.value,
        unit: def?.unit,
      });
      return {
        id: `chip-${f.field}-${f.op}-${index}`,
        text,
        title: text,
        onRemove: () => removeFilterAt(index),
      };
    });
    const trimmedQ = q.trim();
    if (trimmedQ) {
      items.push({
        id: "chip-q",
        text: trimmedQ,
        title: `${t("listQueryBar.hover.activeQ")}: ${trimmedQ}`,
        onRemove: clearQ,
      });
    }
    return items;
  }, [
    clearQ,
    fieldMap,
    filters,
    q,
    removeFilterAt,
    resolveFieldLabel,
    resolveOpLabel,
    t,
  ]);

  const primaryChip = committedChipItems[0] ?? null;
  const overflowCount = Math.max(0, committedChipItems.length - 1);
  const hasCommittedChips = committedChipItems.length > 0;
  const resolvedPlaceholder = hasCommittedChips
    ? ""
    : placeholder?.trim() || defaultCatalogPlaceholder;

  const suggestItems = useMemo<SuggestItem[]>(() => {
    if (!suggestOpen) return [];
    return buildSuggestItems({
      catalog,
      t: translateKey,
      draftText,
      opLabel: resolveOpLabel,
      opDetail: resolveOpDetail,
      maps: aliasMaps,
    });
  }, [
    aliasMaps,
    catalog,
    draftText,
    resolveOpDetail,
    resolveOpLabel,
    suggestOpen,
    translateKey,
  ]);

  useEffect(() => {
    if (!suggestItems.length) {
      setSuggestIndex(-1);
      return;
    }
    setSuggestIndex((prev) =>
      prev < 0 || prev >= suggestItems.length ? 0 : prev,
    );
  }, [suggestItems]);

  const updateSuggestPosition = useCallback(() => {
    const el = shellRef.current;
    if (!el || typeof window === "undefined") return;
    const rect = el.getBoundingClientRect();
    const width = Math.round(Math.max(rect.width, 260));
    const maxLeft = Math.max(8, window.innerWidth - width - 8);
    const left = Math.round(Math.min(Math.max(8, rect.left), maxLeft));
    const top = Math.round(rect.bottom + 4);
    setSuggestCoords((prev) =>
      prev && prev.top === top && prev.left === left && prev.width === width
        ? prev
        : { top, left, width },
    );
  }, []);

  useEffect(() => {
    if (!suggestOpen || !suggestItems.length) {
      return;
    }
    updateSuggestPosition();
    const onViewportChange = () => updateSuggestPosition();
    window.addEventListener("resize", onViewportChange);
    window.addEventListener("scroll", onViewportChange, true);
    let ro: ResizeObserver | null = null;
    if (typeof ResizeObserver !== "undefined" && shellRef.current) {
      ro = new ResizeObserver(onViewportChange);
      ro.observe(shellRef.current);
    }
    return () => {
      window.removeEventListener("resize", onViewportChange);
      window.removeEventListener("scroll", onViewportChange, true);
      ro?.disconnect();
    };
  }, [
    suggestOpen,
    suggestItems.length,
    updateSuggestPosition,
    hasCommittedChips,
  ]);

  useEffect(() => {
    if (!suggestOpen) return;
    const onPointerDown = (event: MouseEvent) => {
      const target = event.target as Node | null;
      if (!target) return;
      if (rootRef.current?.contains(target)) return;
      if (suggestRef.current?.contains(target)) return;
      setSuggestOpen(false);
      setSuggestIndex(-1);
    };
    document.addEventListener("mousedown", onPointerDown, true);
    return () => {
      document.removeEventListener("mousedown", onPointerDown, true);
    };
  }, [suggestOpen]);

  const formatParseError = useCallback(
    (code: string, params?: Record<string, string | number>) => {
      const key = `listQueryBar.parse.${code}`;
      const msg = t(key, params ?? {});
      return msg === key ? code : msg;
    },
    [t],
  );

  const afterFilterCommit = useCallback(() => {
    clearLiveCommitTimer();
    setDraftText("");
    setParseError(null);
    setSuggestOpen(true);
    setSuggestIndex(0);
    requestAnimationFrame(() => {
      inputRef.current?.focus();
      updateSuggestPosition();
    });
  }, [clearLiveCommitTimer, updateSuggestPosition]);

  const commitParsedFilter = useCallback(
    (filter: ListFilter): boolean => {
      const next = mergeByFieldOp([...filtersRef.current], filter);
      const limit = maxFilters(catalog);
      if (next.length > limit) {
        showWarningToast(t("listFilters.tooMany", { max: limit }));
        return false;
      }
      scheduleParentSearch();
      onFiltersChange(next);
      afterFilterCommit();
      return true;
    },
    [afterFilterCommit, catalog, onFiltersChange, scheduleParentSearch, t],
  );

  const tryLiveCommit = useCallback(
    (rawDraft?: string): boolean => {
      if (!liveCommit || composingRef.current) return false;
      const text = (rawDraft ?? draftRef.current).trim();
      if (!text) return false;

      const fieldLocked = matchFieldRefAtStart(text, aliasMaps) !== null;
      if (looksLikeStrongClause(text, catalog, translateKey, aliasMaps)) {
        const result = parseClause(text, catalog, translateKey, aliasMaps);
        if (!result.ok) return false;
        clearLiveCommitTimer();
        return commitParsedFilter(result.filter);
      }

      if (fieldLocked) {
        return false;
      }

      if (qRef.current.trim() === text) {
        afterFilterCommit();
        return true;
      }
      clearLiveCommitTimer();
      scheduleParentSearch();
      onQChange(text);
      afterFilterCommit();
      return true;
    },
    [
      afterFilterCommit,
      aliasMaps,
      catalog,
      clearLiveCommitTimer,
      commitParsedFilter,
      liveCommit,
      onQChange,
      scheduleParentSearch,
      translateKey,
    ],
  );

  const scheduleLiveCommit = useCallback(() => {
    clearLiveCommitTimer();
    if (!liveCommit) return;
    liveCommitTimerRef.current = setTimeout(() => {
      liveCommitTimerRef.current = null;
      tryLiveCommit();
    }, LIVE_COMMIT_MS);
  }, [clearLiveCommitTimer, liveCommit, tryLiveCommit]);

  const commitDraft = useCallback(() => {
    const text = draftRef.current.trim();
    if (!text) {
      onSearch();
      return;
    }

    if (looksLikeStrongClause(text, catalog, translateKey, aliasMaps)) {
      const result = parseClause(text, catalog, translateKey, aliasMaps);
      if (result.ok) {
        commitParsedFilter(result.filter);
        return;
      }
      setParseError(formatParseError(result.code, result.messageParams));
      setSuggestOpen(true);
      setSuggestIndex(0);
      return;
    }

    if (qRef.current.trim() === text) {
      setDraftText("");
      setParseError(null);
      onSearch();
      return;
    }

    scheduleParentSearch();
    onQChange(text);
    afterFilterCommit();
  }, [
    afterFilterCommit,
    aliasMaps,
    catalog,
    commitParsedFilter,
    formatParseError,
    onQChange,
    onSearch,
    scheduleParentSearch,
    translateKey,
  ]);

  const applySuggest = useCallback(
    (item: SuggestItem) => {
      if (item.apply.type === "noop") return;
      if (item.apply.type === "set_draft") {
        clearLiveCommitTimer();
        const nextDraft = item.apply.draft;
        draftRef.current = nextDraft;
        setDraftText(nextDraft);
        setParseError(null);

        if (tryLiveCommit(nextDraft)) {
          return;
        }
        setSuggestOpen(true);
        setSuggestIndex(0);
        requestAnimationFrame(() => {
          inputRef.current?.focus();
          updateSuggestPosition();
        });
      }
    },
    [clearLiveCommitTimer, tryLiveCommit, updateSuggestPosition],
  );

  const handleInputKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (composingRef.current) return;

    if (event.key === "ArrowDown") {
      if (!suggestItems.length) {
        setSuggestOpen(true);
        return;
      }
      event.preventDefault();
      setSuggestOpen(true);
      setSuggestIndex((prev) =>
        prev < 0 ? 0 : (prev + 1) % suggestItems.length,
      );
      return;
    }

    if (event.key === "ArrowUp") {
      if (!suggestItems.length) return;
      event.preventDefault();
      setSuggestIndex((prev) =>
        prev <= 0 ? suggestItems.length - 1 : prev - 1,
      );
      return;
    }

    if (event.key === "Escape") {
      event.preventDefault();
      setSuggestOpen(false);
      setSuggestIndex(-1);
      return;
    }

    if (event.key === "Backspace" && draftText === "") {
      if (filters.length > 0) {
        event.preventDefault();
        removeFilterAt(filters.length - 1);
      } else if (q.trim()) {
        event.preventDefault();
        clearQ();
      }
      return;
    }

    if (event.key === "Enter") {
      event.preventDefault();
      clearLiveCommitTimer();
      const selected =
        suggestOpen && suggestIndex >= 0
          ? suggestItems[suggestIndex]
          : undefined;
      if (selected && selected.apply.type === "set_draft") {
        applySuggest(selected);
        return;
      }
      commitDraft();
    }
  };

  const handleFieldMouseDown = (event: React.MouseEvent<HTMLDivElement>) => {
    const target = event.target as HTMLElement | null;
    if (!target) return;
    if (target.closest("button")) return;
    if (target === inputRef.current) return;
    event.preventDefault();
    inputRef.current?.focus();
    setSuggestOpen(true);
  };

  const suggestPortal =
    suggestOpen &&
    suggestItems.length > 0 &&
    suggestCoords &&
    typeof document !== "undefined"
      ? createPortal(
          <ul
            ref={suggestRef}
            role="listbox"
            data-testid="list-query-bar-suggest"
            style={{
              position: "fixed",
              top: suggestCoords.top,
              left: suggestCoords.left,
              width: suggestCoords.width,
            }}
            className="z-[60] max-h-[min(320px,50vh)] overflow-y-auto rounded-lg border border-border bg-card py-1 text-card-foreground shadow-lg"
          >
            {suggestItems.map((item, idx) => {
              const isActive = idx === suggestIndex;
              const isHint = item.kind === "hint";
              return (
                <li
                  key={item.id}
                  role="option"
                  aria-selected={isActive}
                  onMouseDown={(e) => {
                    e.preventDefault();
                    applySuggest(item);
                  }}
                  onMouseEnter={() => setSuggestIndex(idx)}
                  className={cn(
                    "flex items-center gap-2.5 px-2.5 py-1.5 text-xs transition-colors",
                    isHint
                      ? "cursor-default text-muted-foreground"
                      : "cursor-pointer",
                    !isHint && isActive && "bg-xblue-soft text-xblue",
                    !isHint && !isActive && "text-foreground hover:bg-hover-bg",
                  )}
                >
                  <span
                    aria-hidden="true"
                    className={cn(
                      "inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-md",
                      item.kind === "field" && "bg-xblue-soft text-xblue",
                      item.kind === "op" && "bg-warning-bg text-warning-fg",
                      item.kind === "enum" && "bg-success-fg/10 text-success-fg",
                      item.kind === "hint" && "bg-muted text-muted-foreground",
                      item.kind === "quick" && "bg-xblue-soft text-xblue",
                    )}
                  >
                    <SuggestKindIcon kind={item.kind} />
                  </span>
                  <span className="truncate font-medium">{item.label}</span>
                  {item.detail && (
                    <span className="ml-auto truncate pl-2 font-mono-tech text-[11px] text-muted-foreground">
                      {item.detail}
                    </span>
                  )}
                </li>
              );
            })}
          </ul>,
          document.body,
        )
      : null;

  return (
    <div
      ref={rootRef}
      data-testid="list-query-bar"
      className={cn(
        "inline-flex items-center gap-2 min-w-0 max-w-full",
        className,
      )}
    >
      <div
        className={cn(
          "relative flex flex-col min-w-0 transition-[width] duration-150",
          hasCommittedChips ? "w-[320px] max-w-full" : "w-[260px] max-w-full",
        )}
      >
        <div
          ref={shellRef}
          onMouseDown={handleFieldMouseDown}
          className="flex h-8 w-full min-w-0 items-center gap-1.5 rounded-md border border-input bg-card px-2.5 text-xs text-foreground transition-colors hover:border-border-hover focus-within:border-primary focus-within:ring-2 focus-within:ring-primary/25"
        >
          <Search className="h-3.5 w-3.5 shrink-0 text-muted-foreground pointer-events-none" />

          {primaryChip && (
            <div className="inline-flex items-center gap-1 min-w-0 max-w-[65%] shrink-0">
              <span
                title={primaryChip.title}
                className="inline-flex h-5.5 max-w-[140px] items-center gap-1 rounded-md bg-xblue-soft pl-2 pr-1 text-[11px] font-medium text-xblue"
              >
                <span className="truncate font-mono-tech">
                  {primaryChip.text}
                </span>
                <button
                  type="button"
                  onClick={primaryChip.onRemove}
                  aria-label={t("listFilters.remove")}
                  className="inline-flex h-4 w-4 shrink-0 items-center justify-center rounded hover:bg-xblue-soft-hover cursor-pointer"
                >
                  <X className="h-3 w-3" />
                </button>
              </span>

              {overflowCount > 0 && (
                <Popover
                  open={overflowOpen}
                  onOpenChange={setOverflowOpen}
                  align="left"
                  contentClassName="w-[300px] p-2.5 space-y-2"
                  trigger={
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        setOverflowOpen((v) => !v);
                      }}
                      className="inline-flex h-5.5 shrink-0 items-center justify-center rounded-md bg-xblue-soft px-1.5 text-[11px] font-semibold font-mono-tech text-xblue hover:bg-xblue-soft-hover cursor-pointer"
                    >
                      +{overflowCount}
                    </button>
                  }
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-[11px] font-semibold text-muted-foreground">
                      {t("listQueryBar.hover.activeFilters", {
                        n: committedChipItems.length,
                      })}
                    </span>
                    <button
                      type="button"
                      onClick={clearAllQuery}
                      className="text-[11px] font-medium text-xblue hover:text-xblue-hover cursor-pointer"
                    >
                      {t("listQueryBar.clearAll")}
                    </button>
                  </div>
                  <div className="flex flex-wrap gap-1.5">
                    {committedChipItems.map((item) => (
                      <span
                        key={`overflow-${item.id}`}
                        title={item.title}
                        className="inline-flex h-6 max-w-full items-center gap-1 rounded-md bg-xblue-soft pl-2 pr-1 text-[11px] font-medium text-xblue"
                      >
                        <span className="truncate font-mono-tech">
                          {item.text}
                        </span>
                        <button
                          type="button"
                          onClick={item.onRemove}
                          aria-label={t("listFilters.remove")}
                          className="inline-flex h-4 w-4 shrink-0 items-center justify-center rounded hover:bg-xblue-soft-hover cursor-pointer"
                        >
                          <X className="h-3 w-3" />
                        </button>
                      </span>
                    ))}
                  </div>
                </Popover>
              )}
            </div>
          )}

          <input
            ref={inputRef}
            type="text"
            value={draftText}
            onFocus={() => {
              setSuggestOpen(true);
              updateSuggestPosition();
            }}
            onChange={(e) => {
              const next = e.target.value;
              setDraftText(next);
              setParseError(null);
              setSuggestOpen(true);
              scheduleLiveCommit();
            }}
            onKeyDown={handleInputKeyDown}
            onCompositionStart={() => {
              composingRef.current = true;
            }}
            onCompositionEnd={() => {
              composingRef.current = false;
              scheduleLiveCommit();
            }}
            placeholder={resolvedPlaceholder}
            aria-label={t("listQueryBar.tooltip")}
            className="min-w-[48px] flex-1 bg-transparent text-xs text-foreground placeholder:text-muted-foreground outline-none"
          />

          {draftText && (
            <button
              type="button"
              onClick={() => {
                clearLiveCommitTimer();
                setDraftText("");
                setParseError(null);
                setSuggestOpen(true);
                inputRef.current?.focus();
              }}
              aria-label={t("common.reset")}
              className="inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-hover-bg hover:text-foreground cursor-pointer"
            >
              <X className="h-3 w-3" />
            </button>
          )}
        </div>

        {parseError && (
          <p
            role="alert"
            className="absolute left-0 top-[calc(100%+4px)] z-20 whitespace-nowrap rounded-md border border-danger-fg/30 bg-card px-2 py-1 text-[11px] text-danger-fg shadow-sm"
          >
            {parseError}
          </p>
        )}
      </div>

      {hasCommittedChips && (
        <button
          type="button"
          onClick={clearAllQuery}
          className="h-7 shrink-0 rounded-md px-1.5 text-xs font-medium text-muted-foreground hover:bg-hover-bg hover:text-foreground transition-colors cursor-pointer"
        >
          {t("listQueryBar.clearAll")}
        </button>
      )}

      {suggestPortal}
    </div>
  );
}

export const ListQueryBar = StructuredFilterBar;
