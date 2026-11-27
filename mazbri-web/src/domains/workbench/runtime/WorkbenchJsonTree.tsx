import React, { useMemo, useState } from "react";
import {
  ChevronDown,
  ChevronRight,
  ChevronsDownUp,
  ChevronsUpDown,
  Copy,
  Search,
  X,
} from "lucide-react";
import { useT } from "@/locales";
import { copyWithFeedback } from "@/utils/ui/copy-feedback";
import { cn, Tooltip } from "@/components/ui";

export interface WorkbenchJsonTreeProps {
  value: unknown;
  className?: string;
}

type ChildEntry = {
  key: string | number;
  value: unknown;
  index: boolean;
  path: string;
};

type ExpandSignal = {
  version: number;
  open: boolean;
};

const SIMPLE_KEY_RE = /^[A-Za-z_$][A-Za-z0-9_$]*$/;

function childJsonPath(parent: string, key: string | number, isIndex: boolean): string {
  if (isIndex) return `${parent}[${key}]`;
  const k = String(key);
  if (SIMPLE_KEY_RE.test(k)) return `${parent}.${k}`;
  return `${parent}[${JSON.stringify(k)}]`;
}

function nodeMatches(key: string | number, value: unknown, q: string): boolean {
  if (!q) return true;
  const needle = q.toLowerCase();
  if (String(key).toLowerCase().includes(needle)) return true;
  if (value == null || typeof value !== "object") {
    return String(value).toLowerCase().includes(needle);
  }
  if (Array.isArray(value)) {
    return value.some((v, i) => nodeMatches(i, v, q));
  }
  return Object.entries(value as Record<string, unknown>).some(([k, v]) =>
    nodeMatches(k, v, q),
  );
}

function leafColorStyle(v: unknown): React.CSSProperties {
  if (v === null) return { color: "var(--wb-json-null, var(--muted-foreground))" };
  switch (typeof v) {
    case "string":
      return { color: "var(--wb-json-string, var(--success-fg))" };
    case "number":
      return { color: "var(--wb-json-number, var(--warning-fg))" };
    case "boolean":
      return { color: "var(--wb-json-bool, var(--primary))" };
    default:
      return { color: "var(--wb-json-null, var(--muted-foreground))" };
  }
}

function formatLeafText(v: unknown): string {
  if (v === null) return "null";
  if (typeof v === "string") return JSON.stringify(v);
  return String(v);
}

interface JsonNodeProps {
  value: unknown;
  label?: string | number | null;
  index?: boolean;
  depth: number;
  path: string;
  filter: string;
  expandSignal: ExpandSignal;
}

const JsonNode: React.FC<JsonNodeProps> = ({
  value,
  label = null,
  index = false,
  depth,
  path,
  filter,
  expandSignal,
}) => {
  const { t } = useT();
  const [open, setOpen] = useState(true);
  const [lastSignalVersion, setLastSignalVersion] = useState(expandSignal.version);

  if (expandSignal.version !== lastSignalVersion) {
    setLastSignalVersion(expandSignal.version);
    setOpen(expandSignal.open);
  }

  const isLeaf = value === null || typeof value !== "object";
  const isArray = Array.isArray(value);
  const hasLabel = label !== null && label !== "";
  const labelText = index ? String(label) : JSON.stringify(String(label ?? ""));

  const children = useMemo<ChildEntry[]>(() => {
    if (Array.isArray(value)) {
      return value.map((v, i) => ({
        key: i,
        value: v,
        index: true,
        path: childJsonPath(path, i, true),
      }));
    }
    if (value && typeof value === "object") {
      return Object.entries(value as Record<string, unknown>).map(([k, v]) => ({
        key: k,
        value: v,
        index: false,
        path: childJsonPath(path, k, false),
      }));
    }
    return [];
  }, [value, path]);

  const visibleChildren = useMemo(() => {
    const q = filter.trim();
    if (!q) return children;
    return children.filter((c) => nodeMatches(c.key, c.value, q));
  }, [children, filter]);

  const openBracket = isArray ? "[" : "{";
  const closeBracket = isArray ? "]" : "}";

  const copyPath = (e: React.MouseEvent) => {
    e.stopPropagation();
    void copyWithFeedback(path);
  };

  if (isLeaf) {
    return (
      <div className="group/row flex items-baseline gap-0.5 leading-relaxed font-mono-tech text-xs text-foreground">
        {hasLabel && (
          <Tooltip content={path} side="top">
            <button
              type="button"
              onClick={copyPath}
              className="inline-flex items-center gap-1 rounded-md px-0.5 -mx-0.5 hover:bg-hover-bg cursor-pointer text-left"
              style={{
                color: index
                  ? "var(--muted-foreground)"
                  : "var(--wb-json-key, var(--primary))",
              }}
              aria-label={`${t("common.actions.copy")} ${path}`}
            >
              <span>{labelText}</span>
            </button>
          </Tooltip>
        )}
        {hasLabel && <span className="text-muted-foreground mr-1">:</span>}
        <span className="break-all" style={leafColorStyle(value)}>
          {formatLeafText(value)}
        </span>
        <button
          type="button"
          onClick={copyPath}
          title={path}
          aria-label={`${t("common.actions.copy")} ${path}`}
          className="ml-1 inline-flex h-4 w-4 items-center justify-center rounded-md text-muted-foreground opacity-0 group-hover/row:opacity-100 hover:bg-hover-bg hover:text-foreground transition-opacity cursor-pointer shrink-0"
        >
          <Copy className="h-2.5 w-2.5" />
        </button>
      </div>
    );
  }

  return (
    <div className="font-mono-tech text-xs leading-relaxed text-foreground">
      <div className="group/row inline-flex items-center gap-0.5">
        {children.length > 0 ? (
          <button
            type="button"
            onClick={() => setOpen((v) => !v)}
            className="inline-flex h-4 w-4 items-center justify-center rounded-md text-muted-foreground hover:bg-hover-bg hover:text-foreground cursor-pointer shrink-0"
            aria-expanded={open}
            aria-label={open ? "collapse" : "expand"}
          >
            {open ? (
              <ChevronDown className="h-3 w-3" />
            ) : (
              <ChevronRight className="h-3 w-3" />
            )}
          </button>
        ) : (
          <span className="inline-block w-4 shrink-0" />
        )}
        {hasLabel && (
          <Tooltip content={path} side="top">
            <button
              type="button"
              onClick={copyPath}
              className="inline-flex items-center rounded-md px-0.5 -mx-0.5 hover:bg-hover-bg cursor-pointer"
              style={{
                color: index
                  ? "var(--muted-foreground)"
                  : "var(--wb-json-key, var(--primary))",
              }}
              aria-label={`${t("common.actions.copy")} ${path}`}
            >
              {labelText}
            </button>
          </Tooltip>
        )}
        {hasLabel && <span className="text-muted-foreground mr-1">:</span>}
        <span className="text-muted-foreground">{openBracket}</span>
        {!open && (
          <>
            <span className="px-1 text-[11px] text-muted-foreground tabular-nums">
              {children.length}
            </span>
            <span className="text-muted-foreground">{closeBracket}</span>
          </>
        )}
        <button
          type="button"
          onClick={copyPath}
          title={path}
          aria-label={`${t("common.actions.copy")} ${path}`}
          className="ml-1 inline-flex h-4 w-4 items-center justify-center rounded-md text-muted-foreground opacity-0 group-hover/row:opacity-100 hover:bg-hover-bg hover:text-foreground transition-opacity cursor-pointer shrink-0"
        >
          <Copy className="h-2.5 w-2.5" />
        </button>
      </div>
      {open && (
        <>
          <div className="pl-4 border-l border-border-subtle ml-2">
            {visibleChildren.map((child) => (
              <JsonNode
                key={`${child.index ? "i" : "k"}-${child.key}`}
                value={child.value}
                label={child.key}
                index={child.index}
                depth={depth + 1}
                path={child.path}
                filter={filter}
                expandSignal={expandSignal}
              />
            ))}
          </div>
          <div className="pl-4 text-muted-foreground">{closeBracket}</div>
        </>
      )}
    </div>
  );
};

export function WorkbenchJsonTree({ value, className }: WorkbenchJsonTreeProps) {
  const { t } = useT();
  const [filterText, setFilterText] = useState("");
  const [expandSignal, setExpandSignal] = useState<ExpandSignal>({
    version: 0,
    open: true,
  });

  const rootVisible = useMemo(() => {
    const q = filterText.trim();
    if (!q) return true;
    return nodeMatches("", value, q);
  }, [filterText, value]);

  const toggleAll = (open: boolean) => {
    setExpandSignal((prev) => ({ version: prev.version + 1, open }));
  };

  return (
    <div
      className={cn(
        "flex flex-col h-full min-h-0 rounded-lg border border-border bg-card overflow-hidden",
        className,
      )}
    >
      <div className="flex items-center gap-1.5 border-b border-border bg-muted/40 px-2.5 py-1.5 shrink-0">
        <Search className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
        <input
          type="search"
          value={filterText}
          onChange={(e) => setFilterText(e.target.value)}
          placeholder={t("upstreamConnector.jsonFilterPlaceholder")}
          aria-label={t("upstreamConnector.jsonFilterPlaceholder")}
          spellCheck={false}
          className="flex-1 min-w-0 bg-transparent text-xs text-foreground placeholder:text-muted-foreground focus:outline-none"
        />
        {filterText && (
          <button
            type="button"
            onClick={() => setFilterText("")}
            aria-label={t("upstreamConnector.clearKv")}
            className="inline-flex h-5 w-5 items-center justify-center rounded-md text-muted-foreground hover:bg-hover-bg hover:text-foreground cursor-pointer"
          >
            <X className="h-3 w-3" />
          </button>
        )}
        <div className="h-3.5 w-px bg-border mx-0.5 shrink-0" />
        <button
          type="button"
          onClick={() => toggleAll(true)}
          title={t("upstreamConnector.expandResponse")}
          aria-label={t("upstreamConnector.expandResponse")}
          className="inline-flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground hover:bg-hover-bg hover:text-foreground cursor-pointer shrink-0"
        >
          <ChevronsUpDown className="h-3.5 w-3.5" />
        </button>
        <button
          type="button"
          onClick={() => toggleAll(false)}
          title={t("upstreamConnector.collapseResponse")}
          aria-label={t("upstreamConnector.collapseResponse")}
          className="inline-flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground hover:bg-hover-bg hover:text-foreground cursor-pointer shrink-0"
        >
          <ChevronsDownUp className="h-3.5 w-3.5" />
        </button>
      </div>

      <div className="flex-1 min-h-0 overflow-auto p-3">
        {filterText.trim() && !rootVisible ? (
          <div className="py-6 text-center text-xs text-muted-foreground">
            {t("upstreamConnector.jsonFilterEmpty")}
          </div>
        ) : (
          <JsonNode
            value={value}
            depth={0}
            path="$"
            filter={filterText}
            expandSignal={expandSignal}
          />
        )}
      </div>
    </div>
  );
}
