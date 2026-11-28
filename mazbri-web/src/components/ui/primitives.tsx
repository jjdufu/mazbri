import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import {
  AlertCircle,
  AlertTriangle,
  Check,
  CheckCircle2,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Eye,
  EyeOff,
  Info,
  Loader2,
  Minus,
  Plus,
  Search,
  X,
} from "lucide-react";
import {
  installMessageGuard,
  registerToastSink,
  type ToastPayload,
  type ToastVariant,
} from "@/utils/ui/message-guard";
import { cn } from "./cn";
import { useUiTx } from "./tx";

export function AutoThemeIcon({
  className,
  ...props
}: React.SVGProps<SVGSVGElement>) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.85}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      className={cn("h-4 w-4 shrink-0", className)}
      {...props}
    >
      <path d="M12 2.2L14.8 5H19V9.2L21.8 12L19 14.8V19H14.8L12 21.8L9.2 19H5V14.8L2.2 12L5 9.2V5H9.2L12 2.2Z" />
      <path d="M9.4 15.3L12 8.3L14.6 15.3" />
      <path d="M10.25 13.35H13.75" />
    </svg>
  );
}

export function Card({
  className,
  ...props
}: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn(
        "rounded-lg border border-border bg-card text-card-foreground shadow-[0_1px_2px_rgba(15,23,42,0.025)] dark:shadow-[inset_0_1px_0_rgba(255,255,255,0.055)]",
        className,
      )}
      {...props}
    />
  );
}

export interface ButtonProps extends React.ComponentPropsWithRef<"button"> {
  variant?: "primary" | "outline" | "ghost" | "danger" | "warning" | "link";
  size?: "xs" | "sm" | "md";
  active?: boolean;
  loading?: boolean;
}

export function Button({
  variant = "primary",
  size = "md",
  active = false,
  loading = false,
  disabled,
  className,
  children,
  ...props
}: ButtonProps) {
  const isDisabled = Boolean(disabled || loading);
  return (
    <button
      disabled={isDisabled}
      className={cn(
        "inline-flex items-center justify-center gap-1.5 rounded-md font-medium whitespace-nowrap shrink-0 transition-all duration-150 ease-[cubic-bezier(0.16,1,0.3,1)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-xblue/45 disabled:pointer-events-none cursor-pointer",
        size === "xs" && "h-6 px-2 text-xs",
        size === "sm" && "h-7 px-2.5 text-xs",
        size === "md" && "h-8 px-3 text-xs",
        variant === "primary" &&
          "bg-primary text-primary-foreground border border-white/12 shadow-[0_1px_2px_rgba(0,0,0,0.14),inset_0_1px_0_rgba(255,255,255,0.2)] hover:bg-primary-hover hover:-translate-y-[1px] hover:shadow-[0_4px_12px_color-mix(in_srgb,var(--primary)_40%,transparent),inset_0_1px_0_rgba(255,255,255,0.28)] active:translate-y-0 active:scale-[0.97] disabled:bg-muted disabled:text-muted-foreground disabled:border-border disabled:shadow-none",
        variant === "outline" &&
          (active
            ? "border border-xblue/45 bg-xblue-soft text-xblue hover:bg-xblue-soft-hover hover:-translate-y-[1px] active:translate-y-0 active:scale-[0.97] disabled:opacity-45"
            : "border border-control-border bg-control-bg text-foreground/95 hover:bg-control-hover hover:text-foreground hover:border-border-hover hover:-translate-y-[1px] hover:shadow-[0_3px_8px_rgba(15,23,42,0.07)] dark:hover:shadow-[0_4px_12px_rgba(0,0,0,0.38)] active:translate-y-0 active:scale-[0.97] active:shadow-none disabled:opacity-50"),
        variant === "ghost" &&
          (active
            ? "bg-xblue-soft text-xblue hover:bg-xblue-soft-hover active:scale-[0.97] disabled:opacity-45"
            : "text-muted-foreground hover:bg-hover-bg hover:text-foreground active:bg-active-bg active:scale-[0.97] disabled:opacity-45"),
        variant === "danger" &&
          "border border-[var(--status-err-border)] bg-[var(--status-err-bg)] text-[var(--status-err-fg)] hover:brightness-110 hover:-translate-y-[1px] active:translate-y-0 active:scale-[0.97] disabled:opacity-45",
        variant === "warning" &&
          "border border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-fg)] hover:brightness-110 hover:-translate-y-[1px] active:translate-y-0 active:scale-[0.97] disabled:opacity-45",
        variant === "link" &&
          "bg-transparent text-xblue hover:bg-xblue-soft hover:text-xblue-hover active:bg-xblue-soft-hover active:scale-[0.97] disabled:opacity-45",
        className,
      )}
      {...props}
    >
      {loading && <Loader2 className="h-3.5 w-3.5 animate-spin shrink-0" />}
      {children}
    </button>
  );
}

export interface IconButtonProps
  extends React.ComponentPropsWithRef<"button"> {
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
        "inline-flex items-center justify-center rounded-md transition-all duration-150 ease-[cubic-bezier(0.16,1,0.3,1)] shrink-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-xblue/45 [&_svg]:transition-transform [&_svg]:duration-150 hover:[&_svg]:scale-108 active:scale-[0.94] disabled:pointer-events-none disabled:opacity-45 cursor-pointer",
        size === "sm" ? "h-7 w-7" : "h-8 w-8",
        active
          ? "bg-xblue-soft text-xblue hover:bg-xblue-soft-hover"
          : "text-foreground/78 hover:text-foreground hover:bg-hover-bg active:bg-active-bg",
        className,
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
        "inline-flex items-center gap-1.5 rounded-md border font-medium whitespace-nowrap shrink-0 transition-all duration-150 ease-[cubic-bezier(0.16,1,0.3,1)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-xblue/45 disabled:pointer-events-none disabled:opacity-45 cursor-pointer",
        size === "xs" ? "h-6 px-2 text-[11px]" : "h-7 px-2.5 text-xs",
        active
          ? "border-xblue/45 bg-xblue-soft text-xblue hover:bg-xblue-soft-hover hover:-translate-y-[1px] active:translate-y-0 active:scale-[0.97]"
          : "border-control-border bg-control-bg text-foreground/95 hover:bg-control-hover hover:border-border-hover hover:text-foreground hover:-translate-y-[1px] active:translate-y-0 active:scale-[0.97]",
        className,
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
        "inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[11px] font-sans font-medium whitespace-nowrap shrink-0",
        variant === "neutral" && "border-border bg-muted text-foreground/90",
        variant === "xblue" && "border-xblue/35 bg-xblue-soft text-xblue",
        variant === "outline" &&
          "border-control-border bg-muted/75 text-foreground/86",
        variant === "success" &&
          "border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-fg)]",
        variant === "warning" &&
          "border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-fg)]",
        variant === "danger" &&
          "border-[var(--status-err-border)] bg-[var(--status-err-bg)] text-[var(--status-err-fg)]",
        className,
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
        className,
      )}
    >
      <span>{label}</span>
      <span className="font-mono-tech font-semibold text-foreground tabular-nums">
        {value}
      </span>
    </span>
  );
}

const STATUS_LABELS: Record<string, [string, string]> = {
  ACTIVE: ["启用", "Active"],
  ENABLED: ["已启用", "Enabled"],
  ONLINE: ["在线", "Online"],
  OK: ["正常", "OK"],
  READY: ["就绪", "Ready"],
  HEALTHY: ["健康", "Healthy"],
  SUCCEEDED: ["成功", "Succeeded"],
  SUCCESS: ["成功", "Success"],
  PUBLISHED: ["已发布", "Published"],
  VERIFIED: ["已校验", "Verified"],
  COMMITTED: ["已提交", "Committed"],
  DONE: ["已完成", "Done"],
  RESOLVED: ["已解决", "Resolved"],
  APPROVED: ["已通过", "Approved"],
  CONNECTED: ["已连接", "Connected"],
  NORMAL: ["正常", "Normal"],
  RUNNING: ["运行中", "Running"],
  IN_PROGRESS: ["进行中", "In Progress"],
  LIVE: ["活跃", "Live"],
  ACK: ["已确认", "Acked"],
  ACKED: ["已确认", "Acked"],
  PROCESSING: ["处理中", "Processing"],
  WARN: ["告警", "Warning"],
  WARNING: ["告警", "Warning"],
  PENDING: ["待处理", "Pending"],
  WAITING_APPROVAL: ["待审批", "Pending Approval"],
  DRAFT: ["草稿", "Draft"],
  DEGRADED: ["降级", "Degraded"],
  SUPPRESSED: ["已抑制", "Suppressed"],
  DENIED: ["拒绝", "Denied"],
  REFUND: ["退款", "Refund"],
  REFUNDED: ["已退款", "Refunded"],
  EXPIRING: ["即将过期", "Expiring"],
  FAILED: ["失败", "Failed"],
  FAIL: ["失败", "Failed"],
  ERROR: ["异常", "Error"],
  CRITICAL: ["严重", "Critical"],
  UNHEALTHY: ["异常", "Unhealthy"],
  OFFLINE: ["离线", "Offline"],
  BLOCKED: ["已拦截", "Blocked"],
  LOCKED: ["已锁定", "Locked"],
  REJECTED: ["已驳回", "Rejected"],
  DISABLED: ["已禁用", "Disabled"],
  SUSPENDED: ["已暂停", "Suspended"],
  REVOKED: ["已吊销", "Revoked"],
  EXPIRED: ["已过期", "Expired"],
  FROZEN: ["已冻结", "Frozen"],
  DELETED: ["已删除", "Deleted"],
};

export function StatusBadge({
  status,
  label,
  className,
  ...props
}: React.HTMLAttributes<HTMLSpanElement> & {
  status: string | null | undefined;
  label?: React.ReactNode;
}) {
  const { tx } = useUiTx();
  const upper = (status ?? "").trim().toUpperCase();
  let variant: "neutral" | "xblue" | "outline" | "success" | "warning" | "danger" =
    "outline";

  if (
    [
      "ONLINE",
      "OK",
      "READY",
      "HEALTHY",
      "SUCCEEDED",
      "SUCCESS",
      "ACTIVE",
      "ENABLED",
      "PUBLISHED",
      "VERIFIED",
      "COMMITTED",
      "DONE",
      "RESOLVED",
      "APPROVED",
      "CONNECTED",
      "NORMAL",
    ].includes(upper)
  ) {
    variant = "success";
  } else if (
    ["RUNNING", "IN_PROGRESS", "LIVE", "ACK", "ACKED", "PROCESSING"].includes(
      upper,
    )
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
      "DENIED",
      "REFUND",
      "REFUNDED",
      "EXPIRING",
    ].includes(upper)
  ) {
    variant = "warning";
  } else if (
    [
      "FAILED",
      "FAIL",
      "ERROR",
      "CRITICAL",
      "UNHEALTHY",
      "OFFLINE",
      "BLOCKED",
      "LOCKED",
      "REJECTED",
      "DISABLED",
      "SUSPENDED",
      "REVOKED",
      "EXPIRED",
      "FROZEN",
      "DELETED",
    ].includes(upper)
  ) {
    variant = "danger";
  } else if (status) {
    variant = "neutral";
  }

  const mapped = STATUS_LABELS[upper];
  const content =
    label !== undefined
      ? label
      : mapped
        ? tx(mapped[0], mapped[1])
        : (status ?? "—");

  return (
    <Badge variant={variant} className={className} {...props}>
      {variant === "success" && (
        <Check aria-hidden="true" className="h-3 w-3 stroke-[2.5] shrink-0" />
      )}
      {variant === "xblue" && (
        <span
          aria-hidden="true"
          className="h-2 w-2 rounded-full border-[1.75px] border-current shrink-0"
        />
      )}
      {variant === "warning" && (
        <AlertTriangle
          aria-hidden="true"
          className="h-3 w-3 stroke-[2.25] shrink-0"
        />
      )}
      {variant === "danger" && (
        <X aria-hidden="true" className="h-3 w-3 stroke-[2.5] shrink-0" />
      )}
      {content}
    </Badge>
  );
}

export function Input({
  className,
  ...props
}: React.ComponentPropsWithRef<"input">) {
  return (
    <input
      className={cn(
        "flex h-8 w-full rounded-md border border-control-border bg-control-bg px-2.5 py-1 text-xs text-foreground placeholder:text-muted-foreground transition-all duration-150 hover:bg-control-hover hover:border-border-hover focus:outline-none focus:bg-card focus:border-xblue focus:ring-2 focus:ring-xblue/25 disabled:opacity-50",
        className,
      )}
      {...props}
    />
  );
}

export function PasswordInput({
  className,
  ...props
}: Omit<React.ComponentPropsWithRef<"input">, "type">) {
  const [visible, setVisible] = useState(false);
  const { tx } = useUiTx();
  const toggleLabel = visible
    ? tx("隐藏密码", "Hide password")
    : tx("显示密码", "Show password");
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
        aria-label={toggleLabel}
        title={toggleLabel}
        className="absolute right-1.5 inline-flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground hover:text-foreground hover:bg-hover-bg cursor-pointer transition-colors"
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
  group?: string,
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
  const { tx } = useUiTx();
  const parsedOptions = useMemo(
    () =>
      explicitOptions
        ? [...explicitOptions]
        : parseSelectChildren(children),
    [explicitOptions, children],
  );

  const [internalVal, setInternalVal] = useState<string>(() =>
    defaultValue !== undefined
      ? String(defaultValue)
      : (parsedOptions[0]?.value ?? ""),
  );

  const currentValue = value !== undefined ? String(value) : internalVal;

  const selectedOption =
    parsedOptions.find((o) => o.value === currentValue) ?? null;

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
        opt.value.toLowerCase().includes(q),
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
      Math.max(120, parsedOptions.length * 30 + (enableSearch ? 44 : 10)),
    );
    const openUpward = spaceBelow < 180 && spaceAbove > spaceBelow;
    const maxHeight = Math.max(
      120,
      Math.min(300, openUpward ? spaceAbove : spaceBelow),
    );
    const minWidth = Math.max(120, Math.round(rect.width));
    const left = Math.max(
      8,
      Math.min(Math.round(rect.left), viewportW - minWidth - 8),
    );
    const top = openUpward
      ? Math.max(
          8,
          Math.round(rect.top - Math.min(desiredHeight, maxHeight) - 4),
        )
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
    [value, onChange, name],
  );

  useEffect(() => {
    if (!open) return;
    updatePosition();
    const selectedIdx = filteredOptions.findIndex(
      (o) => o.value === currentValue,
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
        filteredOptions.length === 0 ? 0 : (prev + 1) % filteredOptions.length,
      );
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActiveIndex((prev) =>
        filteredOptions.length === 0
          ? 0
          : (prev - 1 + filteredOptions.length) % filteredOptions.length,
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
          "inline-flex h-8 items-center justify-between gap-2 rounded-md border border-control-border bg-control-bg px-2.5 py-1 text-xs text-foreground transition-all duration-150 hover:bg-control-hover hover:border-border-hover focus:outline-none focus:bg-card focus:ring-2 focus:ring-xblue/25 disabled:opacity-50 disabled:pointer-events-none cursor-pointer text-left",
          open && "bg-card ring-2 ring-xblue/25 border-xblue",
          className,
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
            open && "rotate-180",
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
            className="rounded-lg border border-border bg-card text-card-foreground shadow-lg overflow-hidden"
          >
            {enableSearch && (
              <div className="flex items-center gap-1.5 border-b border-border px-2.5 py-1.5 bg-muted/40">
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
                  placeholder={tx("搜索…", "Filter…")}
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
                            ? "bg-primary-soft text-primary font-medium hover:bg-primary-soft-hover"
                            : highlighted
                              ? "bg-hover-bg text-foreground"
                              : "text-foreground hover:bg-hover-bg",
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
                          <Check className="h-3.5 w-3.5 text-primary shrink-0" />
                        )}
                      </button>
                    </React.Fragment>
                  );
                })
              )}
            </div>
          </div>,
          document.body,
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
        "flex w-full min-w-0 h-8 items-center rounded-md border border-control-border bg-control-bg overflow-hidden hover:bg-control-hover hover:border-border-hover focus-within:bg-card focus-within:border-xblue focus-within:ring-2 focus-within:ring-xblue/25 transition-all duration-150",
        disabled && "opacity-50 pointer-events-none",
        className,
      )}
    >
      <button
        type="button"
        tabIndex={-1}
        disabled={disabled || (min !== undefined && value <= min)}
        onClick={() => onChange(clamp(Number((value - step).toFixed(6))))}
        className="inline-flex h-full w-6 items-center justify-center text-muted-foreground hover:bg-hover-bg hover:text-foreground active:bg-active-bg disabled:opacity-35 cursor-pointer shrink-0 transition-colors"
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
        step="any"
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
        className="inline-flex h-full w-6 items-center justify-center text-muted-foreground hover:bg-hover-bg hover:text-foreground active:bg-active-bg disabled:opacity-35 cursor-pointer shrink-0 transition-colors"
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
        "relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors focus:outline-none focus:ring-2 focus:ring-primary/40 disabled:opacity-45 disabled:pointer-events-none cursor-pointer",
        checked ? "bg-primary" : "bg-muted border border-border",
      )}
    >
      <span
        className={cn(
          "inline-block h-3.5 w-3.5 rounded-full bg-white shadow-xs transition-transform",
          checked ? "translate-x-4.5" : "translate-x-0.5",
        )}
      />
    </button>
  );
}

export function Textarea({
  className,
  ...props
}: React.ComponentPropsWithRef<"textarea">) {
  return (
    <textarea
      className={cn(
        "flex w-full rounded-md border border-control-border bg-control-bg px-3 py-2 text-xs text-foreground placeholder:text-muted-foreground transition-all duration-150 hover:bg-control-hover hover:border-border-hover focus:outline-none focus:bg-card focus:border-xblue focus:ring-2 focus:ring-xblue/25 disabled:opacity-50",
        className,
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
        "flex flex-wrap items-center justify-between gap-3 pb-1",
        className,
      )}
    >
      <div className="min-w-0 space-y-0.5">
        <div className="flex flex-wrap items-center gap-2.5">
          <h1 className="text-base font-semibold tracking-tight text-foreground">
            {title}
          </h1>
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
      <div className="text-xl font-semibold font-mono-tech tabular-nums tracking-tight text-foreground">
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
        "py-12 px-4 text-center space-y-3",
        className,
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

export type TabsVariant = "default" | "inline";

export function Tabs<K extends string>({
  items,
  value,
  onChange,
  variant = "default",
  className,
}: {
  items: readonly TabOption<K>[];
  value: K;
  onChange: (next: K) => void;
  variant?: TabsVariant;
  className?: string;
}) {
  const isInline = variant === "inline";
  return (
    <div
      role="tablist"
      className={cn(
        isInline
          ? "inline-flex max-w-full items-center gap-1 border-b border-border-subtle overflow-x-auto"
          : "flex items-center gap-1 border-b border-border-subtle overflow-x-auto",
        className,
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
              "inline-flex items-center gap-1.5 rounded-t-md border-b-2 text-xs font-medium whitespace-nowrap shrink-0 transition-all duration-150 disabled:opacity-40 disabled:pointer-events-none cursor-pointer -mb-px [&_svg]:transition-colors",
              isInline ? "px-2.5 py-1.5" : "px-2.5 py-2",
              active
                ? "border-xblue text-xblue font-semibold"
                : "border-transparent text-muted-foreground hover:text-foreground hover:bg-nav-hover-bg hover:border-border-hover hover:[&_svg]:text-xblue",
            )}
          >
            {item.icon}
            <span>{item.label}</span>
            {typeof item.count === "number" && (
              <span
                className={cn(
                  "rounded-full px-1.5 py-0.2 text-[10px] font-mono-tech tabular-nums",
                  active
                    ? "bg-primary-soft text-primary"
                    : "bg-muted text-muted-foreground",
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
        "inline-flex h-8 items-center gap-0.5 rounded-md border border-border-subtle bg-muted/85 p-0.5",
        className,
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
              "inline-flex h-full items-center gap-1.5 rounded-md px-2.5 text-xs whitespace-nowrap shrink-0 transition-all duration-150 ease-[cubic-bezier(0.16,1,0.3,1)] disabled:opacity-40 cursor-pointer [&_svg]:transition-colors",
              active
                ? "bg-card text-foreground font-semibold shadow-[0_1px_2px_rgba(15,23,42,0.08)] dark:bg-control-hover [&_svg]:text-xblue"
                : "font-medium text-muted-foreground hover:text-foreground hover:bg-nav-hover-bg hover:[&_svg]:text-xblue active:scale-[0.97]",
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
  pageSizeOptions,
  onPageSizeChange,
  className,
}: {
  page: number;
  pageSize: number;
  total: number;
  onPageChange: (nextPage: number) => void;
  pageSizeOptions?: readonly number[];
  onPageSizeChange?: (nextPageSize: number) => void;
  className?: string;
}) {
  const { tx } = useUiTx();
  const safeSize = Math.max(1, pageSize);
  const totalPages = Math.max(1, Math.ceil(total / safeSize));
  const formattedTotal = total.toLocaleString();

  if (total <= 0 && !onPageSizeChange) return null;

  const sizeItems: SelectOptionItem[] | null =
    pageSizeOptions && onPageSizeChange
      ? pageSizeOptions.map((n) => ({
          value: String(n),
          label: tx(`${n} 条/页`, `${n} / page`),
          text: tx(`${n} 条/页`, `${n} / page`),
        }))
      : null;

  return (
    <div
      className={cn(
        "flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground",
        className,
      )}
    >
      <span className="tabular-nums">
        {tx(
          `共 ${formattedTotal} 条 · 第 ${page} / ${totalPages} 页`,
          `${formattedTotal} items · Page ${page} of ${totalPages}`,
        )}
      </span>
      <div className="flex items-center gap-1.5">
        {sizeItems && onPageSizeChange && (
          <Select
            value={String(pageSize)}
            options={sizeItems}
            onChange={(e) => {
              const nextSize = Number(e.target.value);
              if (Number.isFinite(nextSize) && nextSize > 0) {
                onPageSizeChange(nextSize);
              }
            }}
            aria-label={tx("每页条数", "Page size")}
            className="w-26"
          />
        )}
        <Button
          type="button"
          variant="outline"
          size="md"
          disabled={page <= 1}
          onClick={() => onPageChange(page - 1)}
        >
          <ChevronLeft className="h-3.5 w-3.5" />
          {tx("上一页", "Prev")}
        </Button>
        <Button
          type="button"
          variant="outline"
          size="md"
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

const OVERLAY_SIZE_CLASSES = {
  sm: "max-w-[480px]",
  md: "max-w-[600px]",
  lg: "max-w-[760px]",
} as const;

export function Modal({
  open,
  onClose,
  title,
  subtitle,
  footer,
  children,
  size = "md",
  closeOnBackdrop = true,
  testId,
  closeTestId,
}: {
  open: boolean;
  onClose: () => void;
  title: React.ReactNode;
  subtitle?: React.ReactNode;
  footer?: React.ReactNode;
  children: React.ReactNode;
  size?: "sm" | "md" | "lg";
  closeOnBackdrop?: boolean;
  testId?: string;
  closeTestId?: string;
}) {
  const { tx } = useUiTx();
  useOverlayEscapeAndLock(open, onClose);
  if (!open || typeof document === "undefined") return null;

  return createPortal(
    <div
      role="dialog"
      aria-modal="true"
      data-slot="modal"
      data-testid={testId}
      onMouseDown={(e) => {
        if (closeOnBackdrop && e.target === e.currentTarget) onClose();
      }}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-[1px] p-4"
    >
      <Card
        className={cn(
          "w-full max-h-[88vh] rounded-lg flex flex-col overflow-hidden shadow-xl",
          OVERLAY_SIZE_CLASSES[size],
        )}
      >
        <div className="flex items-start justify-between gap-3 border-b border-border px-4 py-3 shrink-0">
          <div className="min-w-0 space-y-0.5">
            <div className="text-sm font-semibold flex items-center gap-2 min-w-0 truncate text-foreground">
              {title}
            </div>
            {subtitle && (
              <div className="text-xs text-muted-foreground truncate">
                {subtitle}
              </div>
            )}
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
          <div className="flex items-center justify-end gap-2 border-t border-border px-4 py-3 bg-card shrink-0">
            {footer}
          </div>
        )}
      </Card>
    </div>,
    document.body,
  );
}

export function Drawer({
  open,
  onClose,
  title,
  subtitle,
  footer,
  children,
  size = "md",
  closeOnBackdrop = true,
  testId,
  closeTestId,
  closeAriaLabel,
}: {
  open: boolean;
  onClose: () => void;
  title: React.ReactNode;
  subtitle?: React.ReactNode;
  footer?: React.ReactNode;
  children: React.ReactNode;
  size?: "sm" | "md" | "lg";
  closeOnBackdrop?: boolean;
  testId?: string;
  closeTestId?: string;
  closeAriaLabel?: string;
}) {
  const { tx } = useUiTx();
  useOverlayEscapeAndLock(open, onClose);
  if (!open || typeof document === "undefined") return null;
  const closeLabel = closeAriaLabel ?? tx("关闭", "Close");

  return createPortal(
    <div
      role="dialog"
      aria-modal="true"
      data-slot="drawer"
      data-testid={testId}
      onMouseDown={(e) => {
        if (closeOnBackdrop && e.target === e.currentTarget) onClose();
      }}
      className="fixed inset-0 z-50 flex justify-end bg-black/45 backdrop-blur-[1px]"
    >
      <div
        className={cn(
          "w-full h-full bg-card text-card-foreground border-l border-border flex flex-col shadow-2xl",
          OVERLAY_SIZE_CLASSES[size],
        )}
      >
        <div className="flex items-start justify-between gap-3 border-b border-border px-4 py-3 shrink-0">
          <div className="min-w-0 space-y-0.5">
            <div className="text-sm font-semibold flex items-center gap-2 min-w-0 truncate text-foreground">
              {title}
            </div>
            {subtitle && (
              <div className="text-xs text-muted-foreground truncate">
                {subtitle}
              </div>
            )}
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
          <div className="flex items-center justify-end gap-2 border-t border-border px-4 py-3 bg-card shrink-0">
            {footer}
          </div>
        )}
      </div>
    </div>,
    document.body,
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
  const contentRef = useRef<HTMLDivElement | null>(null);
  const [coords, setCoords] = useState<{
    top: number;
    left?: number;
    right?: number;
  } | null>(null);

  const updateCoords = useCallback(() => {
    const el = rootRef.current;
    if (!el || typeof window === "undefined") return;
    const rect = el.getBoundingClientRect();
    const top = Math.round(rect.bottom + 6);
    if (align === "right") {
      const right = Math.max(8, Math.round(window.innerWidth - rect.right));
      setCoords({ top, right });
    } else {
      const left = Math.max(8, Math.round(rect.left));
      setCoords({ top, left });
    }
  }, [align]);

  useEffect(() => {
    if (!open) {
      setCoords(null);
      return;
    }
    updateCoords();
    const onDown = (ev: MouseEvent) => {
      const target = ev.target as Node | null;
      if (
        target &&
        !rootRef.current?.contains(target) &&
        !contentRef.current?.contains(target)
      ) {
        onOpenChange(false);
      }
    };
    const onKey = (ev: KeyboardEvent) => {
      if (ev.key === "Escape") {
        ev.stopPropagation();
        onOpenChange(false);
      }
    };
    const onViewportChange = () => updateCoords();
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    window.addEventListener("resize", onViewportChange);
    window.addEventListener("scroll", onViewportChange, true);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("resize", onViewportChange);
      window.removeEventListener("scroll", onViewportChange, true);
    };
  }, [open, onOpenChange, updateCoords]);

  const portalContent =
    open && coords && typeof document !== "undefined"
      ? createPortal(
          <div
            ref={contentRef}
            data-testid={contentTestId}
            style={{
              position: "fixed",
              top: coords.top,
              left: coords.left,
              right: coords.right,
            }}
            className={cn(
              "z-[60] rounded-lg border border-border bg-card text-card-foreground shadow-lg",
              contentClassName,
            )}
          >
            {children}
          </div>,
          document.body,
        )
      : null;

  return (
    <div ref={rootRef} className={cn("relative inline-flex", className)}>
      {trigger}
      {portalContent}
    </div>
  );
}

export interface TooltipProps {
  content: React.ReactNode;
  disabled?: boolean;
  side?: "top" | "bottom" | "left" | "right";
  children: React.ReactNode;
  className?: string;
}

export function Tooltip({
  content,
  disabled = false,
  side = "top",
  children,
  className,
}: TooltipProps) {
  const [visible, setVisible] = useState(false);
  const [coords, setCoords] = useState<{ top: number; left: number }>({
    top: 0,
    left: 0,
  });
  const anchorRef = useRef<HTMLSpanElement | null>(null);

  const updateCoords = useCallback(() => {
    const el = anchorRef.current;
    if (!el || typeof window === "undefined") return;
    const rect = el.getBoundingClientRect();
    const gap = 8;
    if (side === "top") {
      setCoords({
        top: Math.round(rect.top - gap),
        left: Math.round(rect.left + rect.width / 2),
      });
    } else if (side === "bottom") {
      setCoords({
        top: Math.round(rect.bottom + gap),
        left: Math.round(rect.left + rect.width / 2),
      });
    } else if (side === "left") {
      setCoords({
        top: Math.round(rect.top + rect.height / 2),
        left: Math.round(rect.left - gap),
      });
    } else {
      setCoords({
        top: Math.round(rect.top + rect.height / 2),
        left: Math.round(rect.right + gap),
      });
    }
  }, [side]);

  useEffect(() => {
    if (disabled) {
      setVisible(false);
    }
  }, [disabled]);

  useEffect(() => {
    if (!visible) return;
    updateCoords();
    window.addEventListener("scroll", updateCoords, true);
    window.addEventListener("resize", updateCoords);
    return () => {
      window.removeEventListener("scroll", updateCoords, true);
      window.removeEventListener("resize", updateCoords);
    };
  }, [visible, updateCoords]);

  if (disabled || content === null || content === undefined || content === "") {
    return <>{children}</>;
  }

  const transformStyle =
    side === "top"
      ? "translate(-50%, -100%)"
      : side === "bottom"
        ? "translate(-50%, 0)"
        : side === "left"
          ? "translate(-100%, -50%)"
          : "translate(0, -50%)";

  return (
    <>
      <span
        ref={anchorRef}
        onMouseEnter={() => {
          updateCoords();
          setVisible(true);
        }}
        onMouseLeave={() => setVisible(false)}
        onPointerDown={() => setVisible(false)}
        onFocus={() => {
          updateCoords();
          setVisible(true);
        }}
        onBlur={() => setVisible(false)}
        className={cn("inline-flex items-center", className)}
      >
        {children}
      </span>
      {visible &&
        typeof document !== "undefined" &&
        createPortal(
          <div
            role="tooltip"
            data-slot="tooltip"
            style={{
              position: "fixed",
              top: `${coords.top}px`,
              left: `${coords.left}px`,
              transform: transformStyle,
              zIndex: 90,
            }}
            className="pointer-events-none max-w-xs rounded-md border border-border bg-card px-2.5 py-1.5 text-xs text-foreground shadow-md"
          >
            {content}
          </div>,
          document.body,
        )}
    </>
  );
}

export function ConfirmDialog({
  open,
  onClose,
  onConfirm,
  title,
  description,
  confirmLabel,
  cancelLabel,
  danger = true,
  loading = false,
}: {
  open: boolean;
  onClose: () => void;
  onConfirm: () => void;
  title: React.ReactNode;
  description: React.ReactNode;
  confirmLabel?: React.ReactNode;
  cancelLabel?: React.ReactNode;
  danger?: boolean;
  loading?: boolean;
}) {
  const { tx } = useUiTx();
  return (
    <Modal
      open={open}
      onClose={onClose}
      size="sm"
      title={
        <>
          <AlertTriangle
            className={cn(
              "h-4 w-4 shrink-0",
              danger ? "text-danger-fg" : "text-warning-fg",
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
            {cancelLabel ?? tx("取消", "Cancel")}
          </Button>
          <Button
            variant={danger ? "danger" : "primary"}
            type="button"
            onClick={onConfirm}
            loading={loading}
          >
            {confirmLabel ??
              (danger ? tx("确认删除", "Delete") : tx("确定", "Confirm"))}
          </Button>
        </>
      }
    >
      <div className="text-muted-foreground leading-relaxed">{description}</div>
    </Modal>
  );
}

interface ToastItem {
  id: number;
  variant: ToastVariant;
  message: string;
}

export function ToastHost() {
  const { tx } = useUiTx();
  const [items, setItems] = useState<ToastItem[]>([]);
  const nextIdRef = useRef(1);
  const timersRef = useRef<Map<number, number>>(new Map());

  const dismiss = useCallback((id: number) => {
    const timer = timersRef.current.get(id);
    if (timer !== undefined) {
      window.clearTimeout(timer);
      timersRef.current.delete(id);
    }
    setItems((prev) => prev.filter((item) => item.id !== id));
  }, []);

  useEffect(() => {
    installMessageGuard();

    const sink = (payload: ToastPayload) => {
      const id = nextIdRef.current++;
      const duration = payload.duration ?? 3000;
      setItems((prev) => {
        const next = [
          ...prev,
          { id, variant: payload.variant, message: payload.message },
        ];
        return next.slice(-5);
      });
      if (duration > 0) {
        const timer = window.setTimeout(() => {
          dismiss(id);
        }, duration);
        timersRef.current.set(id, timer);
      }
    };

    const clearSink = (variant?: ToastVariant) => {
      setItems((prev) => {
        const kept: ToastItem[] = [];
        for (const item of prev) {
          if (!variant || item.variant === variant) {
            const timer = timersRef.current.get(item.id);
            if (timer !== undefined) {
              window.clearTimeout(timer);
              timersRef.current.delete(item.id);
            }
          } else {
            kept.push(item);
          }
        }
        return kept;
      });
    };

    registerToastSink(sink, clearSink);
    return () => {
      registerToastSink(null, null);
      for (const timer of timersRef.current.values()) {
        window.clearTimeout(timer);
      }
      timersRef.current.clear();
    };
  }, [dismiss]);

  if (items.length === 0 || typeof document === "undefined") return null;

  return createPortal(
    <div
      data-slot="toast-host"
      aria-live="polite"
      className="fixed top-4 left-1/2 -translate-x-1/2 z-[100] flex flex-col items-center gap-2 pointer-events-none w-full max-w-md px-4"
    >
      {items.map((item) => (
        <div
          key={item.id}
          role="status"
          data-toast-variant={item.variant}
          className={cn(
            "pointer-events-auto flex items-center gap-2.5 w-auto max-w-full rounded-lg border bg-card px-3.5 py-2.5 text-xs text-foreground shadow-lg",
            item.variant === "success" && "border-success-fg/40",
            item.variant === "error" && "border-danger-fg/45",
            item.variant === "warning" && "border-warning-border",
            item.variant === "info" && "border-primary/40",
          )}
        >
          {item.variant === "success" && (
            <CheckCircle2 className="h-4 w-4 shrink-0 text-success-fg" />
          )}
          {item.variant === "error" && (
            <AlertCircle className="h-4 w-4 shrink-0 text-danger-fg" />
          )}
          {item.variant === "warning" && (
            <AlertTriangle className="h-4 w-4 shrink-0 text-warning-fg" />
          )}
          {item.variant === "info" && (
            <Info className="h-4 w-4 shrink-0 text-primary" />
          )}
          <span className="leading-snug break-words">{item.message}</span>
          <button
            type="button"
            onClick={() => dismiss(item.id)}
            aria-label={tx("关闭", "Close")}
            className="ml-1 inline-flex h-5 w-5 items-center justify-center rounded-md text-muted-foreground hover:text-foreground hover:bg-hover-bg shrink-0 cursor-pointer transition-colors"
          >
            <X className="h-3.5 w-3.5" />
          </button>
        </div>
      ))}
    </div>,
    document.body,
  );
}
