import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import {
  AlertTriangle,
  Check,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Eye,
  EyeOff,
  Minus,
  Plus,
  Search,
  X,
} from "lucide-react";
import { translateEnum, useI18n } from "@/i18n";
import { cn } from "@/lib/utils";

export function Card({
  className,
  ...props
}: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn(
        "rounded-lg border border-border bg-card text-card-foreground",
        className
      )}
      {...props}
    />
  );
}

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "outline" | "ghost" | "danger";
  size?: "sm" | "md" | "lg";
  active?: boolean;
}

export function Button({
  variant = "primary",
  size = "md",
  active = false,
  className,
  ...props
}: ButtonProps) {
  return (
    <button
      className={cn(
        "inline-flex items-center justify-center gap-1.5 rounded-md font-medium whitespace-nowrap shrink-0 transition-all duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-xblue/45 active:scale-[0.98] disabled:pointer-events-none cursor-pointer",
        size === "sm" && "h-7 px-2.5 text-xs",
        size === "md" && "h-8 px-3 text-xs",
        size === "lg" && "h-9 px-4 text-sm",
        variant === "primary" &&
          "bg-xblue text-white hover:bg-xblue-hover hover:shadow-sm shadow-2xs disabled:bg-muted disabled:text-muted-foreground disabled:border disabled:border-border disabled:shadow-none",
        variant === "outline" &&
          (active
            ? "border border-xblue/60 bg-xblue-soft text-xblue hover:bg-xblue-soft-hover hover:border-xblue disabled:opacity-45"
            : "border border-border bg-card text-foreground hover:bg-hover-bg hover:border-border-hover active:bg-active-bg disabled:opacity-45"),
        variant === "ghost" &&
          (active
            ? "bg-xblue-soft text-xblue hover:bg-xblue-soft-hover disabled:opacity-40"
            : "text-muted-foreground hover:bg-hover-bg hover:text-foreground active:bg-active-bg disabled:opacity-40"),
        variant === "danger" &&
          "border border-red-500/45 bg-red-500/10 text-red-600 dark:text-red-400 hover:bg-red-500/22 hover:border-red-500/75 disabled:opacity-45",
        className
      )}
      {...props}
    />
  );
}

export interface IconButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  size?: "sm" | "md";
  active?: boolean;
}

export function IconButton({
  size = "md",
  active = false,
  className,
  ...props
}: IconButtonProps) {
  return (
    <button
      type="button"
      className={cn(
        "inline-flex items-center justify-center rounded-md transition-all duration-150 shrink-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-xblue/45 active:scale-[0.96] disabled:pointer-events-none disabled:opacity-40 cursor-pointer",
        size === "sm" ? "h-7 w-7" : "h-8 w-8",
        active
          ? "bg-xblue-soft text-xblue border border-xblue/45 hover:bg-xblue-soft-hover hover:border-xblue"
          : "text-muted-foreground hover:text-foreground hover:bg-hover-bg active:bg-active-bg",
        className
      )}
      {...props}
    />
  );
}

export interface ActionChipProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  active?: boolean;
  size?: "xs" | "sm";
}

export function ActionChip({
  active = false,
  size = "sm",
  className,
  ...props
}: ActionChipProps) {
  return (
    <button
      type="button"
      className={cn(
        "inline-flex items-center gap-1.5 rounded-md border font-medium whitespace-nowrap shrink-0 transition-all duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-xblue/45 disabled:pointer-events-none disabled:opacity-45 cursor-pointer",
        size === "xs" ? "h-6 px-2 text-[11px]" : "h-7 px-2.5 text-xs",
        active
          ? "border-xblue/60 bg-xblue-soft text-xblue hover:bg-xblue-soft-hover hover:border-xblue"
          : "border-border bg-card text-foreground/90 hover:bg-hover-bg hover:border-border-hover hover:text-foreground active:bg-active-bg",
        className
      )}
      {...props}
    />
  );
}

export function Badge({
  variant = "neutral",
  className,
  ...props
}: React.HTMLAttributes<HTMLSpanElement> & {
  variant?: "neutral" | "xblue" | "outline" | "success" | "warning" | "danger";
}) {
  return (
    <span
      className={cn(
        "inline-flex items-center rounded px-1.5 py-0.5 text-[11px] font-sans font-medium whitespace-nowrap shrink-0",
        variant === "neutral" && "bg-muted/80 text-muted-foreground",
        variant === "xblue" && "bg-xblue-soft text-xblue border border-xblue/25",
        variant === "outline" && "border border-border/80 text-muted-foreground",
        variant === "success" &&
          "bg-emerald-500/12 text-emerald-700 dark:text-emerald-400 border border-emerald-500/25",
        variant === "warning" &&
          "bg-amber-500/12 text-amber-700 dark:text-amber-400 border border-amber-500/25",
        variant === "danger" &&
          "bg-red-500/12 text-red-700 dark:text-red-400 border border-red-500/25",
        className
      )}
      {...props}
    />
  );
}

export function StatText({
  label,
  value,
  className,
}: {
  label: React.ReactNode;
  value: React.ReactNode;
  className?: string;
}) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 text-xs text-muted-foreground whitespace-nowrap",
        className
      )}
    >
      <span>{label}</span>
      <span className="font-mono-tech font-semibold text-foreground tabular-nums">
        {value}
      </span>
    </span>
  );
}

export function StatusBadge({
  status,
  className,
  ...props
}: React.HTMLAttributes<HTMLSpanElement> & {
  status: string | null | undefined;
}) {
  const { locale } = useI18n();
  const upper = (status ?? "").toUpperCase();
  let variant: "neutral" | "xblue" | "outline" | "success" | "warning" | "danger" =
    "outline";
  if (
    [
      "ONLINE",
      "OK",
      "READY",
      "HEALTHY",
      "SUCCEEDED",
      "ACTIVE",
      "ENABLED",
      "PUBLISHED",
      "VERIFIED",
      "COMMITTED",
      "DONE",
      "RESOLVED",
      "APPROVED",
      "CONNECTED",
    ].includes(upper)
  ) {
    variant = "success";
  } else if (
    ["RUNNING", "IN_PROGRESS", "LIVE", "ACK", "ACKED"].includes(upper)
  ) {
    variant = "xblue";
  } else if (
    [
      "WARN",
      "WARNING",
      "PENDING",
      "WAITING_APPROVAL",
      "DRAFT",
      "DEGRADED",
      "SUPPRESSED",
    ].includes(upper)
  ) {
    variant = "warning";
  } else if (
    [
      "FAILED",
      "ERROR",
      "CRITICAL",
      "UNHEALTHY",
      "OFFLINE",
      "BLOCKED",
      "LOCKED",
      "REJECTED",
    ].includes(upper)
  ) {
    variant = "danger";
  } else if (status) {
    variant = "neutral";
  }

  return (
    <Badge variant={variant} className={className} {...props}>
      {translateEnum(status, locale)}
    </Badge>
  );
}

export function Input({
  className,
  ...props
}: React.InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      className={cn(
        "flex h-8 w-full rounded-md border border-input bg-card px-2.5 py-1 text-xs placeholder:text-muted-foreground transition-colors hover:border-border-hover focus:outline-none focus:border-xblue focus:ring-2 focus:ring-xblue/30 disabled:opacity-50",
        className
      )}
      {...props}
    />
  );
}

export function PasswordInput({
  className,
  ...props
}: Omit<React.InputHTMLAttributes<HTMLInputElement>, "type">) {
  const [visible, setVisible] = useState(false);
  const { tx } = useI18n();
  return (
    <div className="relative flex items-center w-full">
      <Input
        {...props}
        type={visible ? "text" : "password"}
        className={cn("pr-9", className)}
      />
      <button
        type="button"
        tabIndex={-1}
        onClick={() => setVisible((v) => !v)}
        aria-label={
          visible
            ? tx("隐藏口令", "Hide password")
            : tx("显示口令", "Show password")
        }
        title={
          visible
            ? tx("隐藏口令", "Hide password")
            : tx("显示口令", "Show password")
        }
        className="absolute right-1.5 inline-flex h-6 w-6 items-center justify-center rounded text-muted-foreground hover:text-foreground cursor-pointer"
      >
        {visible ? (
          <EyeOff className="h-3.5 w-3.5" />
        ) : (
          <Eye className="h-3.5 w-3.5" />
        )}
      </button>
    </div>
  );
}

export interface SelectOptionItem {
  readonly value: string;
  readonly label: React.ReactNode;
  readonly text: string;
  readonly disabled?: boolean;
  readonly dotColor?: string;
  readonly icon?: React.ReactNode;
  readonly group?: string;
}

function flattenNodeText(node: React.ReactNode): string {
  if (node === null || node === undefined || typeof node === "boolean") {
    return "";
  }
  if (typeof node === "string" || typeof node === "number") {
    return String(node);
  }
  if (Array.isArray(node)) {
    return node.map(flattenNodeText).join("");
  }
  if (React.isValidElement<{ children?: React.ReactNode }>(node)) {
    return flattenNodeText(node.props.children);
  }
  return "";
}

interface ParsedOptionElementProps {
  value?: string | number;
  disabled?: boolean;
  label?: string;
  children?: React.ReactNode;
  "data-dot"?: string;
}

function parseSelectChildren(
  children: React.ReactNode,
  group?: string
): SelectOptionItem[] {
  const out: SelectOptionItem[] = [];
  React.Children.forEach(children, (child) => {
    if (!React.isValidElement<ParsedOptionElementProps>(child)) {
      return;
    }
    if (child.type === React.Fragment) {
      out.push(...parseSelectChildren(child.props.children, group));
      return;
    }
    if (child.type === "optgroup") {
      const nextGroup =
        typeof child.props.label === "string" ? child.props.label : group;
      out.push(...parseSelectChildren(child.props.children, nextGroup));
      return;
    }
    if (child.type === "option") {
      const text = flattenNodeText(child.props.children).trim();
      const rawValue =
        child.props.value !== undefined ? String(child.props.value) : text;
      out.push({
        value: rawValue,
        label: child.props.children ?? rawValue,
        text: text || rawValue,
        disabled: Boolean(child.props.disabled),
        dotColor: child.props["data-dot"],
        group,
      });
    }
  });
  return out;
}

export interface CustomSelectProps
  extends Omit<
    React.SelectHTMLAttributes<HTMLSelectElement>,
    "multiple" | "size"
  > {
  options?: readonly SelectOptionItem[];
  searchable?: boolean;
  placeholder?: string;
  "data-testid"?: string;
}

export function Select({
  className,
  children,
  options: explicitOptions,
  value,
  defaultValue,
  onChange,
  disabled,
  searchable,
  placeholder,
  id,
  name,
  title,
  "aria-label": ariaLabel,
  "data-testid": testId,
}: CustomSelectProps) {
  const { tx } = useI18n();
  const parsedOptions = useMemo(
    () =>
      explicitOptions
        ? [...explicitOptions]
        : parseSelectChildren(children),
    [explicitOptions, children]
  );

  const [internalVal, setInternalVal] = useState<string>(() =>
    defaultValue !== undefined
      ? String(defaultValue)
      : (parsedOptions[0]?.value ?? "")
  );

  const currentValue =
    value !== undefined ? String(value) : internalVal;

  const selectedOption =
    parsedOptions.find((o) => o.value === currentValue) ??
    parsedOptions[0] ??
    null;

  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const [menuPos, setMenuPos] = useState<{
    top: number;
    left: number;
    minWidth: number;
    maxHeight: number;
  }>({ top: 0, left: 0, minWidth: 160, maxHeight: 280 });

  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const menuRef = useRef<HTMLDivElement | null>(null);
  const searchInputRef = useRef<HTMLInputElement | null>(null);

  const enableSearch = searchable ?? parsedOptions.length > 10;

  const filteredOptions = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return parsedOptions;
    return parsedOptions.filter(
      (opt) =>
        opt.text.toLowerCase().includes(q) ||
        opt.value.toLowerCase().includes(q)
    );
  }, [parsedOptions, query]);

  const updatePosition = useCallback(() => {
    const btn = triggerRef.current;
    if (!btn || typeof window === "undefined") return;
    const rect = btn.getBoundingClientRect();
    const viewportH = window.innerHeight;
    const viewportW = window.innerWidth;
    const spaceBelow = viewportH - rect.bottom - 12;
    const spaceAbove = rect.top - 12;
    const desiredHeight = Math.min(
      300,
      Math.max(120, parsedOptions.length * 30 + (enableSearch ? 44 : 10))
    );
    const openUpward = spaceBelow < 180 && spaceAbove > spaceBelow;
    const maxHeight = Math.max(
      120,
      Math.min(300, openUpward ? spaceAbove : spaceBelow)
    );
    const minWidth = Math.max(140, Math.round(rect.width));
    const left = Math.max(
      8,
      Math.min(Math.round(rect.left), viewportW - minWidth - 8)
    );
    const top = openUpward
      ? Math.max(8, Math.round(rect.top - Math.min(desiredHeight, maxHeight) - 4))
      : Math.round(rect.bottom + 4);
    setMenuPos({ top, left, minWidth, maxHeight });
  }, [parsedOptions.length, enableSearch]);

  const commitValue = useCallback(
    (nextValue: string) => {
      if (value === undefined) {
        setInternalVal(nextValue);
      }
      setOpen(false);
      setQuery("");
      if (onChange) {
        const syntheticEvent = {
          target: { value: nextValue, name: name ?? "" },
          currentTarget: { value: nextValue, name: name ?? "" },
          preventDefault: () => {},
          stopPropagation: () => {},
        } as unknown as React.ChangeEvent<HTMLSelectElement>;
        onChange(syntheticEvent);
      }
    },
    [value, onChange, name]
  );

  useEffect(() => {
    if (!open) return;
    updatePosition();
    const selectedIdx = filteredOptions.findIndex(
      (o) => o.value === currentValue
    );
    setActiveIndex(selectedIdx >= 0 ? selectedIdx : 0);
    if (enableSearch) {
      requestAnimationFrame(() => {
        searchInputRef.current?.focus();
      });
    }
    const onMouseDown = (ev: MouseEvent) => {
      const target = ev.target as Node | null;
      if (
        target &&
        !triggerRef.current?.contains(target) &&
        !menuRef.current?.contains(target)
      ) {
        setOpen(false);
        setQuery("");
      }
    };
    const onKeyDown = (ev: KeyboardEvent) => {
      if (ev.key === "Escape") {
        ev.stopPropagation();
        setOpen(false);
        setQuery("");
        triggerRef.current?.focus();
      }
    };
    const onViewportChange = () => updatePosition();
    window.addEventListener("mousedown", onMouseDown);
    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("resize", onViewportChange);
    window.addEventListener("scroll", onViewportChange, true);
    return () => {
      window.removeEventListener("mousedown", onMouseDown);
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("resize", onViewportChange);
      window.removeEventListener("scroll", onViewportChange, true);
    };
  }, [open, updatePosition, enableSearch, filteredOptions, currentValue]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (disabled) return;
    if (!open) {
      if (e.key === "Enter" || e.key === " " || e.key === "ArrowDown") {
        e.preventDefault();
        setOpen(true);
      }
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActiveIndex((prev) =>
        filteredOptions.length === 0 ? 0 : (prev + 1) % filteredOptions.length
      );
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActiveIndex((prev) =>
        filteredOptions.length === 0
          ? 0
          : (prev - 1 + filteredOptions.length) % filteredOptions.length
      );
    } else if (e.key === "Enter") {
      e.preventDefault();
      const targetOpt = filteredOptions[activeIndex];
      if (targetOpt && !targetOpt.disabled) {
        commitValue(targetOpt.value);
      }
    }
  };

  return (
    <>
      <button
        ref={triggerRef}
        id={id}
        type="button"
        role="combobox"
        aria-expanded={open}
        aria-haspopup="listbox"
        aria-label={ariaLabel}
        title={title ?? selectedOption?.text}
        disabled={disabled}
        data-testid={testId}
        data-value={currentValue}
        onClick={() => {
          if (!disabled) {
            setOpen((v) => !v);
          }
        }}
        onKeyDown={handleKeyDown}
        className={cn(
          "inline-flex h-8 items-center justify-between gap-2 rounded-md border border-input bg-card px-2.5 py-1 text-xs text-foreground transition-all duration-150 hover:bg-hover-bg hover:border-border-hover focus:outline-none focus:ring-2 focus:ring-xblue/35 disabled:opacity-50 disabled:pointer-events-none cursor-pointer text-left",
          open && "ring-2 ring-xblue/35 border-xblue",
          className
        )}
      >
        <span className="inline-flex items-center gap-1.5 min-w-0 truncate">
          {selectedOption?.dotColor && (
            <span
              className="h-2 w-2 rounded-full shrink-0"
              style={{ backgroundColor: selectedOption.dotColor }}
            />
          )}
          {selectedOption?.icon && (
            <span className="shrink-0 text-muted-foreground">
              {selectedOption.icon}
            </span>
          )}
          <span className="truncate">
            {selectedOption ? (
              selectedOption.label
            ) : (
              <span className="text-muted-foreground">
                {placeholder ?? tx("请选择", "Select")}
              </span>
            )}
          </span>
        </span>
        <ChevronDown
          className={cn(
            "h-3.5 w-3.5 text-muted-foreground shrink-0 transition-transform",
            open && "rotate-180"
          )}
        />
      </button>

      {open &&
        typeof document !== "undefined" &&
        createPortal(
          <div
            ref={menuRef}
            role="listbox"
            aria-label={ariaLabel}
            data-testid={testId ? `${testId}-listbox` : undefined}
            style={{
              position: "fixed",
              top: `${menuPos.top}px`,
              left: `${menuPos.left}px`,
              minWidth: `${menuPos.minWidth}px`,
              maxWidth: "min(420px, calc(100vw - 16px))",
              zIndex: 80,
            }}
            className="rounded-md border border-border bg-card text-card-foreground shadow-lg overflow-hidden"
          >
            {enableSearch && (
              <div className="flex items-center gap-1.5 border-b border-border px-2.5 py-1.5 bg-muted/20">
                <Search className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
                <input
                  ref={searchInputRef}
                  type="text"
                  value={query}
                  onChange={(e) => {
                    setQuery(e.target.value);
                    setActiveIndex(0);
                  }}
                  onKeyDown={handleKeyDown}
                  placeholder={tx("搜索选项…", "Filter options…")}
                  className="w-full bg-transparent text-xs text-foreground placeholder:text-muted-foreground focus:outline-none"
                />
              </div>
            )}
            <div
              style={{ maxHeight: `${menuPos.maxHeight}px` }}
              className="overflow-y-auto p-1 space-y-0.5"
            >
              {filteredOptions.length === 0 ? (
                <div className="px-2.5 py-3 text-center text-xs text-muted-foreground">
                  {tx("无匹配项", "No matches")}
                </div>
              ) : (
                filteredOptions.map((opt, idx) => {
                  const selected = opt.value === currentValue;
                  const highlighted = idx === activeIndex;
                  const prevGroup =
                    idx > 0 ? filteredOptions[idx - 1]?.group : undefined;
                  const showGroup = opt.group && opt.group !== prevGroup;
                  return (
                    <React.Fragment key={`${opt.value}-${idx}`}>
                      {showGroup && (
                        <div className="px-2 pt-1.5 pb-0.5 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                          {opt.group}
                        </div>
                      )}
                      <button
                        type="button"
                        role="option"
                        aria-selected={selected}
                        disabled={opt.disabled}
                        data-option-value={opt.value}
                        onMouseEnter={() => setActiveIndex(idx)}
                        onClick={() => {
                          if (!opt.disabled) {
                            commitValue(opt.value);
                          }
                        }}
                        className={cn(
                          "w-full flex items-center justify-between gap-2 rounded-md px-2 py-1.5 text-xs text-left transition-colors disabled:opacity-40 disabled:pointer-events-none cursor-pointer",
                          selected
                            ? "bg-xblue-soft text-xblue font-medium hover:bg-xblue-soft-hover"
                            : highlighted
                              ? "bg-hover-bg text-foreground"
                              : "text-foreground hover:bg-hover-bg"
                        )}
                      >
                        <span className="inline-flex items-center gap-1.5 min-w-0 truncate">
                          {opt.dotColor && (
                            <span
                              className="h-2 w-2 rounded-full shrink-0"
                              style={{ backgroundColor: opt.dotColor }}
                            />
                          )}
                          {opt.icon && (
                            <span className="shrink-0">{opt.icon}</span>
                          )}
                          <span className="truncate">{opt.label}</span>
                        </span>
                        {selected && (
                          <Check className="h-3.5 w-3.5 text-xblue shrink-0" />
                        )}
                      </button>
                    </React.Fragment>
                  );
                })
              )}
            </div>
          </div>,
          document.body
        )}
    </>
  );
}

export function NumberInput({
  value,
  onChange,
  min,
  max,
  step = 1,
  disabled,
  className,
  "aria-label": ariaLabel,
  "data-testid": testId,
}: {
  value: number;
  onChange: (next: number) => void;
  min?: number;
  max?: number;
  step?: number;
  disabled?: boolean;
  className?: string;
  "aria-label"?: string;
  "data-testid"?: string;
}) {
  const clamp = (n: number): number => {
    let next = n;
    if (min !== undefined && next < min) next = min;
    if (max !== undefined && next > max) next = max;
    return next;
  };

  return (
    <div
      className={cn(
        "flex w-full min-w-0 h-8 items-center rounded-md border border-input bg-background overflow-hidden focus-within:ring-1 focus-within:ring-xblue",
        disabled && "opacity-50 pointer-events-none",
        className
      )}
    >
      <button
        type="button"
        tabIndex={-1}
        disabled={disabled || (min !== undefined && value <= min)}
        onClick={() => onChange(clamp(Number((value - step).toFixed(6))))}
        className="inline-flex h-full w-5 items-center justify-center text-muted-foreground hover:bg-hover-bg hover:text-foreground active:bg-active-bg disabled:opacity-35 cursor-pointer shrink-0 transition-colors"
      >
        <Minus className="h-3 w-3" />
      </button>
      <input
        type="number"
        aria-label={ariaLabel}
        data-testid={testId}
        disabled={disabled}
        min={min}
        max={max}
        step={step}
        value={Number.isFinite(value) ? value : 0}
        onChange={(e) => {
          const parsed = Number(e.target.value);
          if (Number.isFinite(parsed)) {
            onChange(clamp(parsed));
          }
        }}
        className="w-full min-w-0 flex-1 bg-transparent px-1 text-center text-xs font-mono-tech tabular-nums text-foreground focus:outline-none"
      />
      <button
        type="button"
        tabIndex={-1}
        disabled={disabled || (max !== undefined && value >= max)}
        onClick={() => onChange(clamp(Number((value + step).toFixed(6))))}
        className="inline-flex h-full w-5 items-center justify-center text-muted-foreground hover:bg-hover-bg hover:text-foreground active:bg-active-bg disabled:opacity-35 cursor-pointer shrink-0 transition-colors"
      >
        <Plus className="h-3 w-3" />
      </button>
    </div>
  );
}

export function Switch({
  checked,
  onCheckedChange,
  disabled,
  id,
  "aria-label": ariaLabel,
  "data-testid": testId,
}: {
  checked: boolean;
  onCheckedChange: (next: boolean) => void;
  disabled?: boolean;
  id?: string;
  "aria-label"?: string;
  "data-testid"?: string;
}) {
  return (
    <button
      id={id}
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={ariaLabel}
      data-testid={testId}
      disabled={disabled}
      onClick={() => onCheckedChange(!checked)}
      className={cn(
        "relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors focus:outline-none focus:ring-2 focus:ring-xblue/40 disabled:opacity-45 disabled:pointer-events-none cursor-pointer",
        checked ? "bg-xblue" : "bg-muted border border-border"
      )}
    >
      <span
        className={cn(
          "inline-block h-3.5 w-3.5 rounded-full bg-white shadow-xs transition-transform",
          checked ? "translate-x-4.5" : "translate-x-0.5"
        )}
      />
    </button>
  );
}

export function Textarea({
  className,
  ...props
}: React.TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return (
    <textarea
      className={cn(
        "flex w-full rounded-md border border-input bg-card px-3 py-2 text-xs placeholder:text-muted-foreground transition-colors hover:border-border-hover focus:outline-none focus:border-xblue focus:ring-2 focus:ring-xblue/30 disabled:opacity-50",
        className
      )}
      {...props}
    />
  );
}

export interface PageHeaderProps {
  title: React.ReactNode;
  subtitle?: React.ReactNode;
  badges?: React.ReactNode;
  actions?: React.ReactNode;
  className?: string;
}

export function PageHeader({
  title,
  subtitle,
  badges,
  actions,
  className,
}: PageHeaderProps) {
  return (
    <div
      className={cn(
        "flex flex-wrap items-center justify-between gap-3 border-b border-border pb-3",
        className
      )}
    >
      <div className="min-w-0 space-y-0.5">
        <div className="flex flex-wrap items-center gap-2.5">
          <h1 className="text-base font-semibold tracking-tight">{title}</h1>
          {badges}
        </div>
        {subtitle && (
          <p className="text-xs text-muted-foreground">{subtitle}</p>
        )}
      </div>
      {actions && (
        <div className="flex flex-wrap items-center gap-2 shrink-0">
          {actions}
        </div>
      )}
    </div>
  );
}

export function StatCard({
  label,
  value,
  hint,
  icon,
  className,
}: {
  label: React.ReactNode;
  value: React.ReactNode;
  hint?: React.ReactNode;
  icon?: React.ReactNode;
  className?: string;
}) {
  return (
    <Card className={cn("p-3.5 space-y-1.5", className)}>
      <div className="flex items-center justify-between gap-2 text-xs text-muted-foreground">
        <span className="truncate">{label}</span>
        {icon && <span className="shrink-0 text-muted-foreground">{icon}</span>}
      </div>
      <div className="text-xl font-semibold font-mono-tech tabular-nums tracking-tight">
        {value}
      </div>
      {hint && <div className="text-[11px] text-muted-foreground">{hint}</div>}
    </Card>
  );
}

export function EmptyState({
  title,
  action,
  className,
}: {
  title: React.ReactNode;
  action?: React.ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "rounded-md border border-dashed border-border py-10 px-4 text-center space-y-3",
        className
      )}
    >
      <div className="text-xs text-muted-foreground">{title}</div>
      {action && <div className="flex justify-center">{action}</div>}
    </div>
  );
}

export interface TabOption<K extends string = string> {
  readonly key: K;
  readonly label: React.ReactNode;
  readonly icon?: React.ReactNode;
  readonly count?: number;
  readonly disabled?: boolean;
  readonly testId?: string;
}

export function Tabs<K extends string>({
  items,
  value,
  onChange,
  className,
}: {
  items: readonly TabOption<K>[];
  value: K;
  onChange: (next: K) => void;
  className?: string;
}) {
  return (
    <div
      role="tablist"
      className={cn(
        "flex items-center gap-1 border-b border-border overflow-x-auto",
        className
      )}
    >
      {items.map((item) => {
        const active = item.key === value;
        return (
          <button
            key={item.key}
            type="button"
            role="tab"
            aria-selected={active}
            disabled={item.disabled}
            data-testid={item.testId}
            onClick={() => onChange(item.key)}
            className={cn(
              "inline-flex items-center gap-1.5 rounded-t-md border-b-2 px-2.5 py-2 text-xs font-medium whitespace-nowrap shrink-0 transition-colors disabled:opacity-40 disabled:pointer-events-none cursor-pointer -mb-px",
              active
                ? "border-xblue text-xblue font-semibold"
                : "border-transparent text-muted-foreground hover:text-foreground hover:bg-hover-bg/75"
            )}
          >
            {item.icon}
            <span>{item.label}</span>
            {typeof item.count === "number" && (
              <span
                className={cn(
                  "rounded px-1.5 py-0.2 text-[10px] font-mono-tech tabular-nums",
                  active
                    ? "bg-xblue-soft text-xblue"
                    : "bg-muted text-muted-foreground"
                )}
              >
                {item.count}
              </span>
            )}
          </button>
        );
      })}
    </div>
  );
}

export function SegmentedControl<K extends string>({
  items,
  value,
  onChange,
  className,
}: {
  items: readonly TabOption<K>[];
  value: K;
  onChange: (next: K) => void;
  className?: string;
}) {
  return (
    <div
      role="tablist"
      className={cn(
        "inline-flex h-8 items-center gap-0.5 rounded-md border border-border bg-muted/50 p-0.5",
        className
      )}
    >
      {items.map((item) => {
        const active = item.key === value;
        return (
          <button
            key={item.key}
            type="button"
            role="tab"
            aria-selected={active}
            disabled={item.disabled}
            data-testid={item.testId}
            onClick={() => onChange(item.key)}
            className={cn(
              "inline-flex h-full items-center gap-1.5 rounded-md px-2.5 text-xs whitespace-nowrap shrink-0 transition-colors disabled:opacity-40 cursor-pointer",
              active
                ? "bg-xblue-soft text-xblue font-semibold shadow-2xs"
                : "font-medium text-muted-foreground hover:text-foreground hover:bg-hover-bg"
            )}
          >
            {item.icon}
            <span>{item.label}</span>
            {typeof item.count === "number" && (
              <span className="text-[10px] font-mono-tech tabular-nums opacity-85">
                {item.count}
              </span>
            )}
          </button>
        );
      })}
    </div>
  );
}

export function Pagination({
  page,
  pageSize,
  total,
  onPageChange,
  className,
}: {
  page: number;
  pageSize: number;
  total: number;
  onPageChange: (nextPage: number) => void;
  className?: string;
}) {
  const { tx, fmtNum } = useI18n();
  const totalPages = Math.max(1, Math.ceil(total / Math.max(1, pageSize)));
  if (total <= pageSize) return null;
  return (
    <div
      className={cn(
        "flex items-center justify-between gap-2 pt-2 text-xs text-muted-foreground",
        className
      )}
    >
      <span>
        {tx(
          `共 ${fmtNum(total, 0)} 条 · 第 ${page} / ${totalPages} 页`,
          `${fmtNum(total, 0)} items · Page ${page} of ${totalPages}`
        )}
      </span>
      <div className="flex items-center gap-1">
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={page <= 1}
          onClick={() => onPageChange(page - 1)}
        >
          <ChevronLeft className="h-3.5 w-3.5" />
          {tx("上一页", "Prev")}
        </Button>
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={page >= totalPages}
          onClick={() => onPageChange(page + 1)}
        >
          {tx("下一页", "Next")}
          <ChevronRight className="h-3.5 w-3.5" />
        </Button>
      </div>
    </div>
  );
}

function useOverlayEscapeAndLock(open: boolean, onClose: () => void): void {
  useEffect(() => {
    if (!open || typeof document === "undefined") return;
    const prevOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    const onKey = (ev: KeyboardEvent) => {
      if (ev.key === "Escape") {
        ev.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      document.body.style.overflow = prevOverflow;
      window.removeEventListener("keydown", onKey);
    };
  }, [open, onClose]);
}

export function Modal({
  open,
  onClose,
  title,
  footer,
  children,
  maxWidthClass = "max-w-lg",
  testId,
  closeTestId,
}: {
  open: boolean;
  onClose: () => void;
  title: React.ReactNode;
  footer?: React.ReactNode;
  children: React.ReactNode;
  maxWidthClass?: string;
  testId?: string;
  closeTestId?: string;
}) {
  const { tx } = useI18n();
  useOverlayEscapeAndLock(open, onClose);
  if (!open || typeof document === "undefined") return null;

  return createPortal(
    <div
      role="dialog"
      aria-modal="true"
      data-testid={testId}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-[1px] p-4"
    >
      <Card
        className={cn(
          "w-full max-h-[88vh] rounded-xl flex flex-col overflow-hidden shadow-xl",
          maxWidthClass
        )}
      >
        <div className="flex items-center justify-between gap-3 border-b border-border px-4 py-3 shrink-0">
          <div className="text-sm font-semibold flex items-center gap-2 min-w-0 truncate">
            {title}
          </div>
          <IconButton
            size="sm"
            onClick={onClose}
            data-testid={closeTestId}
            aria-label={tx("关闭", "Close")}
            title={tx("关闭", "Close")}
          >
            <X className="h-4 w-4" />
          </IconButton>
        </div>
        <div className="flex-1 overflow-y-auto p-4 space-y-3 text-xs">
          {children}
        </div>
        {footer && (
          <div className="flex items-center justify-end gap-2 border-t border-border px-4 py-3 bg-muted/20 shrink-0">
            {footer}
          </div>
        )}
      </Card>
    </div>,
    document.body
  );
}

export function Drawer({
  open,
  onClose,
  title,
  footer,
  children,
  widthClass = "max-w-xl",
  testId,
  closeTestId,
  closeAriaLabel,
}: {
  open: boolean;
  onClose: () => void;
  title: React.ReactNode;
  footer?: React.ReactNode;
  children: React.ReactNode;
  widthClass?: string;
  testId?: string;
  closeTestId?: string;
  closeAriaLabel?: string;
}) {
  const { tx } = useI18n();
  useOverlayEscapeAndLock(open, onClose);
  if (!open || typeof document === "undefined") return null;
  const closeLabel = closeAriaLabel ?? tx("关闭", "Close");

  return createPortal(
    <div
      role="dialog"
      aria-modal="true"
      data-testid={testId}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
      className="fixed inset-0 z-50 flex justify-end bg-black/45 backdrop-blur-[1px]"
    >
      <div
        className={cn(
          "w-full h-full bg-card text-card-foreground border-l border-border flex flex-col shadow-2xl",
          widthClass
        )}
      >
        <div className="flex items-center justify-between gap-3 border-b border-border px-4 py-3 shrink-0">
          <div className="text-sm font-semibold flex items-center gap-2 min-w-0 truncate">
            {title}
          </div>
          <IconButton
            size="sm"
            onClick={onClose}
            data-testid={closeTestId}
            aria-label={closeLabel}
            title={closeLabel}
          >
            <X className="h-4 w-4" />
          </IconButton>
        </div>
        <div className="flex-1 overflow-y-auto p-4 space-y-3 text-xs">
          {children}
        </div>
        {footer && (
          <div className="flex items-center justify-end gap-2 border-t border-border px-4 py-3 bg-muted/20 shrink-0">
            {footer}
          </div>
        )}
      </div>
    </div>,
    document.body
  );
}

export interface PopoverProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  trigger: React.ReactNode;
  children: React.ReactNode;
  align?: "left" | "right";
  className?: string;
  contentClassName?: string;
  contentTestId?: string;
}

export function Popover({
  open,
  onOpenChange,
  trigger,
  children,
  align = "right",
  className,
  contentClassName,
  contentTestId,
}: PopoverProps) {
  const rootRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (ev: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(ev.target as Node)) {
        onOpenChange(false);
      }
    };
    const onKey = (ev: KeyboardEvent) => {
      if (ev.key === "Escape") {
        ev.stopPropagation();
        onOpenChange(false);
      }
    };
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [open, onOpenChange]);

  return (
    <div ref={rootRef} className={cn("relative inline-flex", className)}>
      {trigger}
      {open && (
        <div
          data-testid={contentTestId}
          className={cn(
            "absolute top-full mt-1.5 z-50 rounded-md border border-border bg-card text-card-foreground shadow-lg",
            align === "right" ? "right-0" : "left-0",
            contentClassName
          )}
        >
          {children}
        </div>
      )}
    </div>
  );
}

export function ConfirmDialog({
  open,
  onClose,
  onConfirm,
  title,
  description,
  confirmLabel,
  danger = true,
  loading = false,
}: {
  open: boolean;
  onClose: () => void;
  onConfirm: () => void;
  title: React.ReactNode;
  description: React.ReactNode;
  confirmLabel?: React.ReactNode;
  danger?: boolean;
  loading?: boolean;
}) {
  const { tx } = useI18n();
  return (
    <Modal
      open={open}
      onClose={onClose}
      maxWidthClass="max-w-md"
      title={
        <>
          <AlertTriangle
            className={cn(
              "h-4 w-4 shrink-0",
              danger ? "text-red-500" : "text-amber-500"
            )}
          />
          <span>{title}</span>
        </>
      }
      footer={
        <>
          <Button
            variant="outline"
            type="button"
            onClick={onClose}
            disabled={loading}
          >
            {tx("取消", "Cancel")}
          </Button>
          <Button
            variant={danger ? "danger" : "primary"}
            type="button"
            onClick={onConfirm}
            disabled={loading}
          >
            {confirmLabel ??
              (danger ? tx("删除", "Delete") : tx("确定", "Confirm"))}
          </Button>
        </>
      }
    >
      <p className="text-muted-foreground leading-relaxed">{description}</p>
    </Modal>
  );
}
