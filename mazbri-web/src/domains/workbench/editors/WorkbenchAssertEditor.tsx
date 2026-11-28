import { useMemo, useState } from "react";
import { GripVertical, Plus, Trash2 } from "lucide-react";
import type { AssertionSpec } from "@/api/upstream-connectors";
import {
  IconButton,
  Input,
  Select,
  cn,
  type SelectOptionItem,
} from "@/components/ui";
import { useT } from "@/locales";
import { ASSERT_TYPES } from "../engine/workbenchHelpers";

export interface WorkbenchAssertEditorProps {
  assertions: AssertionSpec[];
  onChange: (next: AssertionSpec[]) => void;
  disabled?: boolean;
}

function newAssertUid(): string {
  if (
    typeof crypto !== "undefined" &&
    typeof crypto.randomUUID === "function"
  ) {
    return crypto.randomUUID();
  }
  return `as-${Date.now().toString(36)}-${Math.random().toString(16).slice(2)}`;
}

function expectedText(a: AssertionSpec): string {
  if (a.expected == null) return "";
  return typeof a.expected === "string"
    ? a.expected
    : JSON.stringify(a.expected);
}

export function WorkbenchAssertEditor({
  assertions,
  onChange,
  disabled = false,
}: WorkbenchAssertEditorProps) {
  const { t } = useT();
  const [dragIdx, setDragIdx] = useState<number | null>(null);

  const typeOptions: SelectOptionItem[] = useMemo(
    () => [
      { value: "", label: "status_range", text: "status_range" },
      ...ASSERT_TYPES.map((type) => ({
        value: type,
        label: type,
        text: type,
      })),
    ],
    [],
  );

  const isTail = (idx: number) =>
    idx === assertions.length - 1 && !(assertions[idx]?.type || "").trim();

  const patchRow = (idx: number, patch: Partial<AssertionSpec>) => {
    const next = assertions.map((row, i) =>
      i === idx ? { ...row, ...patch } : row,
    );
    onChange(next);
  };

  const add = () => {
    onChange([...assertions, { type: "status_range", min: 200, max: 299, uid: newAssertUid() }]);
  };

  const clear = () => {
    onChange([]);
  };

  const remove = (idx: number) => {
    onChange(assertions.filter((_, i) => i !== idx));
  };

  const handleDrop = (targetIdx: number) => {
    if (dragIdx === null || dragIdx === targetIdx || isTail(targetIdx)) {
      setDragIdx(null);
      return;
    }
    const next = [...assertions];
    const [moved] = next.splice(dragIdx, 1);
    if (moved) {
      next.splice(targetIdx, 0, moved);
      onChange(next);
    }
    setDragIdx(null);
  };

  const parseNum = (raw: string): number => {
    const n = Number(raw);
    return Number.isFinite(n) ? n : 0;
  };

  return (
    <div className="flex flex-col min-h-0">
      <div className="flex items-center justify-between gap-2 min-h-8 px-1 pb-1.5 shrink-0">
        <span className="text-xs font-medium text-muted-foreground">
          {t("upstreamConnector.assertTab")}
        </span>
        <div className="flex items-center gap-0.5">

          <IconButton
            size="sm"
            disabled={disabled || !assertions.length}
            title={t("upstreamConnector.clearKv")}
            aria-label={t("upstreamConnector.clearKv")}
            onClick={clear}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </IconButton>

          <IconButton
            size="sm"
            disabled={disabled}
            title={t("upstreamConnector.addAssertion")}
            aria-label={t("upstreamConnector.addAssertion")}
            onClick={add}
          >
            <Plus className="h-3.5 w-3.5" />
          </IconButton>
        </div>
      </div>

      {assertions.length > 0 && (
        <div className="w-full overflow-hidden rounded-lg border border-border bg-card">
          {assertions.map((a, idx) => {
            const tail = isTail(idx);
            const rowKey = a.uid || `assert-${idx}`;
            return (
              <div
                key={rowKey}
                draggable={!disabled && !tail}
                onDragStart={() => setDragIdx(idx)}
                onDragOver={(e) => {
                  if (!disabled && !tail) e.preventDefault();
                }}
                onDrop={() => handleDrop(idx)}
                onDragEnd={() => setDragIdx(null)}
                className={cn(
                  "group grid grid-cols-[28px_minmax(152px,0.75fr)_minmax(0,1fr)_minmax(0,1fr)_32px] items-center gap-1.5 border-b border-border-subtle px-1.5 py-1.5 last:border-b-0",
                  dragIdx === idx && "bg-hover-bg opacity-70",
                )}
              >
                <div className="flex items-center justify-center">
                  {!tail && !disabled && (
                    <span
                      title={t("upstreamConnector.kvDrag")}
                      className="inline-flex h-6 w-6 items-center justify-center text-muted-foreground opacity-0 group-hover:opacity-100 cursor-grab active:cursor-grabbing"
                    >
                      <GripVertical className="h-3.5 w-3.5" />
                    </span>
                  )}
                </div>

                <Select
                  value={a.type || ""}
                  options={typeOptions}
                  searchable
                  disabled={disabled}
                  placeholder="status_range"
                  aria-label={t("upstreamConnector.fields.assertType")}
                  className="w-full font-mono-tech"
                  onChange={(e) => patchRow(idx, { type: e.target.value })}
                />

                {a.type === "status_range" ? (
                  <>
                    <Input
                      type="number"
                      min={0}
                      max={599}
                      disabled={disabled}
                      value={a.min ?? ""}
                      placeholder="200"
                      aria-label="status_range min"
                      className="font-mono-tech tabular-nums"
                      onChange={(e) =>
                        patchRow(idx, { min: parseNum(e.target.value) })
                      }
                    />
                    <Input
                      type="number"
                      min={0}
                      max={599}
                      disabled={disabled}
                      value={a.max ?? ""}
                      placeholder="299"
                      aria-label="status_range max"
                      className="font-mono-tech tabular-nums"
                      onChange={(e) =>
                        patchRow(idx, { max: parseNum(e.target.value) })
                      }
                    />
                  </>
                ) : a.type === "status_in" ? (
                  <div className="col-span-2">
                    <Input
                      disabled={disabled}
                      value={(a.values || []).join(",")}
                      placeholder="200,201"
                      aria-label="status_in values"
                      className="font-mono-tech"
                      onChange={(e) => {
                        const values = e.target.value
                          .split(/[,\s]+/)
                          .map((s) => Number(s))
                          .filter((n) => Number.isFinite(n));
                        patchRow(idx, { values });
                      }}
                    />
                  </div>
                ) : a.type === "latency_ms_lte" ? (
                  <div className="col-span-2">
                    <Input
                      type="number"
                      min={0}
                      disabled={disabled}
                      value={Number(a.value || 0)}
                      placeholder="1500"
                      aria-label="latency_ms_lte"
                      className="font-mono-tech tabular-nums"
                      onChange={(e) =>
                        patchRow(idx, { value: parseNum(e.target.value) })
                      }
                    />
                  </div>
                ) : a.type?.startsWith("jsonpath") ? (
                  <>
                    <div
                      className={cn(
                        a.type !== "jsonpath_equals" &&
                          a.type !== "jsonpath_matches" &&
                          "col-span-2",
                      )}
                    >
                      <Input
                        disabled={disabled}
                        value={a.path || ""}
                        placeholder="$.data.id"
                        aria-label="jsonpath"
                        className="font-mono-tech"
                        onChange={(e) =>
                          patchRow(idx, { path: e.target.value })
                        }
                      />
                    </div>
                    {a.type === "jsonpath_equals" && (
                      <Input
                        disabled={disabled}
                        value={expectedText(a)}
                        placeholder="expected"
                        aria-label="jsonpath expected"
                        className="font-mono-tech"
                        onChange={(e) => {
                          const raw = e.target.value;
                          try {
                            patchRow(idx, { expected: JSON.parse(raw) });
                          } catch {
                            patchRow(idx, { expected: raw });
                          }
                        }}
                      />
                    )}
                    {a.type === "jsonpath_matches" && (
                      <Input
                        disabled={disabled}
                        value={a.pattern || ""}
                        placeholder="^[A-Z0-9]+$"
                        aria-label="jsonpath pattern"
                        className="font-mono-tech"
                        onChange={(e) =>
                          patchRow(idx, { pattern: e.target.value })
                        }
                      />
                    )}
                  </>
                ) : a.type === "body_contains" ||
                  a.type === "body_not_contains" ? (
                  <div className="col-span-2">
                    <Input
                      disabled={disabled}
                      value={String(a.value ?? a.expected ?? "")}
                      placeholder="ok"
                      aria-label="body substring"
                      className="font-mono-tech"
                      onChange={(e) => patchRow(idx, { value: e.target.value })}
                    />
                  </div>
                ) : a.type === "header_equals" || a.type === "header_exists" ? (
                  <>
                    <div
                      className={cn(a.type === "header_exists" && "col-span-2")}
                    >
                      <Input
                        disabled={disabled}
                        value={a.key || a.path || ""}
                        placeholder="Content-Type"
                        aria-label="header name"
                        className="font-mono-tech"
                        onChange={(e) => patchRow(idx, { key: e.target.value })}
                      />
                    </div>
                    {a.type === "header_equals" && (
                      <Input
                        disabled={disabled}
                        value={String(a.value ?? a.expected ?? "")}
                        placeholder="application/json"
                        aria-label="header value"
                        className="font-mono-tech"
                        onChange={(e) =>
                          patchRow(idx, { value: e.target.value })
                        }
                      />
                    )}
                  </>
                ) : (
                  <div className="col-span-2" />
                )}

                <div className="flex items-center justify-center">
                  <button
                    type="button"
                    disabled={disabled}
                    onClick={() => remove(idx)}
                    title={t("common.actions.delete")}
                    aria-label={t("common.actions.delete")}
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
  );
}
