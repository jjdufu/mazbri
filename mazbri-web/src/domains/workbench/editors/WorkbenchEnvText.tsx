import React, {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import { cn } from "@/components/ui";
import { useT } from "@/locales";
import {
  applyEnvVarCompletion,
  filterEnvKeys,
  incompleteEnvVar,
  splitEnvText,
  type EnvTextSegment,
} from "../engine/workbenchHelpers";

export interface WorkbenchEnvTextHandle {
  focus: () => void;
  blur: () => void;
}

export interface WorkbenchEnvTextProps {
  value: string;
  onChange: (next: string) => void;
  env?: Record<string, string>;
  placeholder?: string;
  ariaLabel?: string;
  disabled?: boolean;
  compact?: boolean;
  secret?: boolean;
  maskPreview?: boolean;
  className?: string;
  inputClassName?: string;
  onFocus?: (ev: React.FocusEvent<HTMLInputElement>) => void;
  onBlur?: (ev: React.FocusEvent<HTMLInputElement>) => void;
  onKeyDown?: (ev: React.KeyboardEvent<HTMLInputElement>) => void;
  onPaste?: (ev: React.ClipboardEvent<HTMLInputElement>) => void;
}

export const WorkbenchEnvText = forwardRef<
  WorkbenchEnvTextHandle,
  WorkbenchEnvTextProps
>(function WorkbenchEnvText(
  {
    value,
    onChange,
    env = {},
    placeholder = "",
    ariaLabel = "",
    disabled = false,
    compact = false,
    secret = false,
    maskPreview = false,
    className,
    inputClassName,
    onFocus,
    onBlur,
    onKeyDown,
    onPaste,
  },
  ref,
) {
  const { t } = useT();
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const overlayRef = useRef<HTMLDivElement | null>(null);

  const [composing, setComposing] = useState(false);
  const [suggestions, setSuggestions] = useState<string[]>([]);
  const [activeIdx, setActiveIdx] = useState(0);
  const [hovered, setHovered] = useState(false);
  const [popupPos, setPopupPos] = useState<{
    top: number;
    left: number;
    width: number;
  }>({ top: 0, left: 0, width: 260 });

  useImperativeHandle(ref, () => ({
    focus: () => inputRef.current?.focus(),
    blur: () => inputRef.current?.blur(),
  }));

  const suggestOpen = suggestions.length > 0;

  const segments = useMemo<EnvTextSegment[]>(() => {
    if (composing) {
      return value ? [{ kind: "text", text: value }] : [];
    }
    return splitEnvText(value || "", env);
  }, [composing, value, env]);

  const varTokens = useMemo(() => {
    const seen = new Set<string>();
    const out: Extract<EnvTextSegment, { kind: "var" }>[] = [];
    for (const seg of segments) {
      if (seg.kind !== "var" || seen.has(seg.name)) continue;
      seen.add(seg.name);
      out.push(seg);
    }
    return out;
  }, [segments]);

  const syncScroll = useCallback(() => {
    const overlay = overlayRef.current;
    const input = inputRef.current;
    if (!overlay || !input) return;
    overlay.scrollLeft = input.scrollLeft;
  }, []);

  useEffect(() => {
    syncScroll();
  }, [value, syncScroll]);

  const measurePopup = useCallback(() => {
    const el = wrapRef.current;
    if (!el || typeof window === "undefined") return;
    const rect = el.getBoundingClientRect();
    const width = Math.max(240, Math.min(340, Math.round(rect.width)));
    const left = Math.max(
      8,
      Math.min(Math.round(rect.left), window.innerWidth - width - 8),
    );
    setPopupPos({
      top: Math.round(rect.bottom + 4),
      left,
      width,
    });
  }, []);

  const updateSuggest = useCallback(
    (el: HTMLInputElement) => {
      const caret = el.selectionStart ?? el.value.length;
      const hit = incompleteEnvVar(el.value, caret);
      if (!hit) {
        setSuggestions([]);
        return;
      }
      const keys = filterEnvKeys(env, hit.prefix);
      setSuggestions(keys);
      setActiveIdx(0);
      if (keys.length > 0) {
        measurePopup();
      }
    },
    [env, measurePopup],
  );

  const pick = useCallback(
    (name: string) => {
      const el = inputRef.current;
      const caret = el?.selectionStart ?? (value || "").length;
      const next = applyEnvVarCompletion(value || "", caret, name);
      if (!next) return;
      onChange(next.text);
      setSuggestions([]);
      requestAnimationFrame(() => {
        const input = inputRef.current;
        if (!input) return;
        input.focus();
        input.setSelectionRange(next.caret, next.caret);
        syncScroll();
      });
    },
    [value, onChange, syncScroll],
  );

  const handleInputChange = (ev: React.ChangeEvent<HTMLInputElement>) => {
    const el = ev.target;
    onChange(el.value);
    syncScroll();
    if (!composing) updateSuggest(el);
  };

  const handleFocus = (ev: React.FocusEvent<HTMLInputElement>) => {
    onFocus?.(ev);
    if (!composing) updateSuggest(ev.currentTarget);
  };

  const handleBlur = (ev: React.FocusEvent<HTMLInputElement>) => {
    setSuggestions([]);
    onBlur?.(ev);
  };

  const handleKeyDown = (ev: React.KeyboardEvent<HTMLInputElement>) => {
    if (suggestOpen) {
      if (ev.key === "ArrowDown") {
        ev.preventDefault();
        setActiveIdx((i) => (i + 1) % suggestions.length);
        return;
      }
      if (ev.key === "ArrowUp") {
        ev.preventDefault();
        setActiveIdx((i) => (i - 1 + suggestions.length) % suggestions.length);
        return;
      }
      if (ev.key === "Enter" || ev.key === "Tab") {
        const name = suggestions[activeIdx];
        if (name) {
          ev.preventDefault();
          pick(name);
          return;
        }
      }
      if (ev.key === "Escape") {
        ev.preventDefault();
        setSuggestions([]);
        return;
      }
    }
    onKeyDown?.(ev);
  };

  const showPreview =
    hovered && !suggestOpen && !secret && varTokens.length > 0;

  return (
    <div
      ref={wrapRef}
      onMouseEnter={() => {
        if (!secret && varTokens.length > 0) {
          measurePopup();
          setHovered(true);
        }
      }}
      onMouseLeave={() => setHovered(false)}
      className={cn(
        "relative w-full min-w-0",
        compact ? "h-8" : "h-full min-h-8",
        className,
      )}
    >
      {!secret && (
        <div
          ref={overlayRef}
          aria-hidden="true"
          className={cn(
            "pointer-events-none absolute inset-0 overflow-hidden whitespace-nowrap text-xs text-foreground",
            compact ? "px-2.5 leading-8" : "px-2.5 leading-8",
            inputClassName,
          )}
        >
          <span className="inline-block whitespace-pre">
            {segments.map((seg, idx) =>
              seg.kind === "var" ? (
                <span
                  key={idx}
                  className={cn(
                    "rounded-md px-0.5 text-primary-foreground",
                    seg.found ? "bg-success-fg" : "bg-danger-fg",
                  )}
                >
                  {seg.raw}
                </span>
              ) : (
                <span key={idx}>{seg.text}</span>
              ),
            )}
          </span>
        </div>
      )}

      <input
        ref={inputRef}
        type={secret ? "password" : "text"}
        value={value}
        placeholder={placeholder}
        aria-label={ariaLabel || placeholder || "input"}
        disabled={disabled}
        spellCheck={false}
        autoComplete="off"
        onChange={handleInputChange}
        onScroll={syncScroll}
        onFocus={handleFocus}
        onBlur={handleBlur}
        onKeyDown={handleKeyDown}
        onKeyUp={(e) => {
          if (!composing) updateSuggest(e.currentTarget);
        }}
        onClick={(e) => {
          if (!composing) updateSuggest(e.currentTarget);
        }}
        onPaste={onPaste}
        onCompositionStart={() => setComposing(true)}
        onCompositionEnd={(e) => {
          setComposing(false);
          onChange(e.currentTarget.value);
          updateSuggest(e.currentTarget);
        }}
        className={cn(
          "relative z-10 h-full w-full min-w-0 bg-transparent px-2.5 text-xs outline-none placeholder:text-muted-foreground disabled:cursor-not-allowed disabled:opacity-50",
          compact ? "leading-8" : "min-h-8 leading-8",
          secret || !value
            ? "text-foreground"
            : "text-transparent caret-foreground",
          inputClassName,
        )}
      />

      {suggestOpen &&
        typeof document !== "undefined" &&
        createPortal(
          <div
            role="listbox"
            style={{
              position: "fixed",
              top: `${popupPos.top}px`,
              left: `${popupPos.left}px`,
              width: `${popupPos.width}px`,
              zIndex: 90,
            }}
            className="max-h-56 overflow-y-auto rounded-lg border border-border bg-card p-1 shadow-lg"
          >
            {suggestions.map((k, i) => (
              <button
                key={k}
                type="button"
                role="option"
                aria-selected={i === activeIdx}
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => pick(k)}
                className={cn(
                  "flex w-full items-center justify-between gap-3 rounded-md px-2 py-1.5 text-left text-xs transition-colors cursor-pointer",
                  i === activeIdx
                    ? "bg-primary-soft text-primary font-medium"
                    : "text-foreground hover:bg-hover-bg",
                )}
              >
                <span className="font-mono-tech truncate">{k}</span>
                <span className="max-w-[140px] truncate text-[11px] text-muted-foreground">
                  {maskPreview ? (env[k] ? "••••••••" : "∅") : env[k] || "∅"}
                </span>
              </button>
            ))}
          </div>,
          document.body,
        )}

      {showPreview &&
        typeof document !== "undefined" &&
        createPortal(
          <div
            style={{
              position: "fixed",
              top: `${popupPos.top}px`,
              left: `${popupPos.left}px`,
              width: `${Math.max(260, popupPos.width)}px`,
              zIndex: 85,
            }}
            className="pointer-events-none rounded-lg border border-border bg-card p-2.5 shadow-lg space-y-1.5"
          >
            {varTokens.map((v) => (
              <div
                key={v.name}
                className="flex items-center justify-between gap-2 text-xs"
              >
                <code
                  className={cn(
                    "font-mono-tech shrink-0",
                    v.found ? "text-success-fg" : "text-danger-fg",
                  )}
                >
                  {v.raw}
                </code>
                {v.found ? (
                  <span className="truncate font-mono-tech text-foreground">
                    {maskPreview
                      ? env[v.name]
                        ? "••••••••"
                        : "∅"
                      : env[v.name] || "∅"}
                  </span>
                ) : (
                  <span className="font-semibold text-danger-fg">
                    {t("upstreamConnector.envUndefined")}
                  </span>
                )}
              </div>
            ))}
          </div>,
          document.body,
        )}
    </div>
  );
});
