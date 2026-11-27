import { useEffect, useState } from "react";
import { AlertTriangle, Plus, Trash2 } from "lucide-react";
import {
  Button,
  CodeEditor,
  IconButton,
  Input,
  Popover,
  SegmentedControl,
  Tooltip,
} from "@/components/ui";
import { useT } from "@/locales";

export interface JsonTemplateEditorProps {
  value: string;
  onChange: (next: string) => void;
  variables?: string[];
  mode?: "form" | "raw";
  onModeChange?: (next: "form" | "raw") => void;
  disabled?: boolean;
  height?: number;
  emptyPlaceholder?: boolean;
  keyColumnLabel?: string;
  valueColumnLabel?: string;
  opColumnLabel?: string;
  keyPlaceholder?: string;
  valuePlaceholder?: string;
}

interface KvItem {
  key: string;
  value: string;
}

function parseLenientJsonTemplate(str: string): KvItem[] | null {
  const trimmed = str.trim();
  if (!trimmed) return [];
  if (!trimmed.startsWith("{") || !trimmed.endsWith("}")) return null;

  const inner = trimmed.slice(1, -1).trim();
  if (!inner) return [];

  const items: KvItem[] = [];
  let i = 0;

  try {
    while (i < inner.length) {
      while (
        i < inner.length &&
        (inner[i] === " " ||
          inner[i] === "\t" ||
          inner[i] === "\n" ||
          inner[i] === "\r" ||
          inner[i] === ",")
      ) {
        i++;
      }
      if (i >= inner.length) break;

      if (inner[i] !== '"') return null;
      i++;
      const keyStart = i;
      while (i < inner.length && inner[i] !== '"') {
        if (inner[i] === "\\") i++;
        i++;
      }
      if (i >= inner.length) return null;
      const key = inner.slice(keyStart, i);
      i++;

      while (
        i < inner.length &&
        (inner[i] === " " ||
          inner[i] === "\t" ||
          inner[i] === "\n" ||
          inner[i] === "\r")
      ) {
        i++;
      }
      if (inner[i] !== ":") return null;
      i++;

      while (
        i < inner.length &&
        (inner[i] === " " ||
          inner[i] === "\t" ||
          inner[i] === "\n" ||
          inner[i] === "\r")
      ) {
        i++;
      }
      if (i >= inner.length) return null;

      if (inner[i] === '"') {
        i++;
        const strStart = i;
        while (i < inner.length && inner[i] !== '"') {
          if (inner[i] === "\\") i++;
          i++;
        }
        if (i >= inner.length) return null;
        items.push({ key, value: inner.slice(strStart, i) });
        i++;
      } else if (inner[i] === "{" && inner[i + 1] === "{") {
        let bracesCount = 0;
        const exprStart = i;
        while (i < inner.length) {
          if (inner[i] === "{" && inner[i + 1] === "{") {
            bracesCount++;
            i += 2;
          } else if (inner[i] === "}" && inner[i + 1] === "}") {
            bracesCount--;
            i += 2;
            if (bracesCount === 0) break;
          } else {
            i++;
          }
        }
        items.push({ key, value: inner.slice(exprStart, i).trim() });
      } else {
        let braceCount = 0;
        let bracketCount = 0;
        const valStart = i;
        while (i < inner.length) {
          const char = inner[i];
          if (char === "{") braceCount++;
          else if (char === "}") braceCount--;
          else if (char === "[") bracketCount++;
          else if (char === "]") bracketCount--;
          else if (char === "," && braceCount === 0 && bracketCount === 0) {
            break;
          }
          i++;
        }
        const val = inner.slice(valStart, i).trim();
        if (
          (val.startsWith("{") && !val.startsWith("{{")) ||
          val.startsWith("[")
        ) {
          return null;
        }
        items.push({ key, value: val });
      }
    }
    return items;
  } catch {
    return null;
  }
}

function serializeKvItems(items: KvItem[]): string {
  const lines = items
    .filter((item) => item.key.trim() !== "")
    .map((item) => {
      const k = JSON.stringify(item.key);
      const v = item.value.trim();
      const isRawExpr = v.startsWith("{{") && v.endsWith("}}");
      const isNested =
        (v.startsWith("{") && v.endsWith("}")) ||
        (v.startsWith("[") && v.endsWith("]"));
      const isNumber = !Number.isNaN(Number(v)) && v !== "";
      const isBoolOrNull = ["true", "false", "null"].includes(v.toLowerCase());
      if (isRawExpr || isNested || isNumber || isBoolOrNull) {
        return `  ${k}: ${v}`;
      }
      return `  ${k}: ${JSON.stringify(v)}`;
    });
  return `{\n${lines.join(",\n")}\n}`;
}

export function JsonTemplateEditor({
  value,
  onChange,
  variables = [],
  mode: controlledMode,
  onModeChange,
  disabled = false,
  height = 180,
  emptyPlaceholder = false,
  keyColumnLabel,
  valueColumnLabel,
  opColumnLabel,
  keyPlaceholder,
  valuePlaceholder,
}: JsonTemplateEditorProps) {
  const { t } = useT();
  const [mode, setMode] = useState<"form" | "raw">(controlledMode ?? "form");
  const [canUseForm, setCanUseForm] = useState(true);
  const [formItems, setFormItems] = useState<KvItem[]>([]);
  const [validationError, setValidationError] = useState<string | null>(null);
  const [varPopoverIndex, setVarPopoverIndex] = useState<number | null>(null);

  const validateRaw = (str: string) => {
    if (!str.trim()) {
      setValidationError(null);
      return;
    }
    let openBraces = 0;
    for (const c of str) {
      if (c === "{") openBraces++;
      else if (c === "}") openBraces--;
    }
    setValidationError(
      openBraces !== 0 ? t("supplier.jsonTemplate.braceMismatch") : null,
    );
  };

  useEffect(() => {
    if (controlledMode && controlledMode !== mode) {
      setMode(controlledMode);
    }
  }, [controlledMode, mode]);

  useEffect(() => {
    const val = value || "";
    if (!val.trim()) {
      setFormItems(emptyPlaceholder ? [] : [{ key: "", value: "" }]);
      setCanUseForm(true);
      setValidationError(null);
      return;
    }
    try {
      const parsed: unknown = JSON.parse(val);
      if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
        setFormItems(
          Object.entries(parsed as Record<string, unknown>).map(([k, v]) => ({
            key: k,
            value: typeof v === "object" ? JSON.stringify(v) : String(v),
          })),
        );
        setCanUseForm(true);
        setValidationError(null);
        return;
      }
    } catch {
      // fall through to lenient parse
    }

    const parsedItems = parseLenientJsonTemplate(val);
    if (parsedItems) {
      setFormItems(parsedItems);
      setCanUseForm(true);
      setValidationError(null);
    } else {
      setCanUseForm(false);
      setMode("raw");
      onModeChange?.("raw");
      validateRaw(val);
    }
  }, [value, emptyPlaceholder]);

  const updateItems = (nextItems: KvItem[]) => {
    setFormItems(nextItems);
    setValidationError(null);
    onChange(serializeKvItems(nextItems));
  };

  const handleModeSwitch = (nextMode: "form" | "raw") => {
    setMode(nextMode);
    onModeChange?.(nextMode);
  };

  const handleFormatJson = () => {
    try {
      const parsed: unknown = JSON.parse(value);
      onChange(JSON.stringify(parsed, null, 2));
      setValidationError(null);
    } catch {
      const parsedItems = parseLenientJsonTemplate(value);
      if (parsedItems) {
        updateItems(parsedItems);
      } else {
        setValidationError(t("supplier.jsonTemplate.formatError"));
      }
    }
  };

  const resolvedKeyCol = keyColumnLabel || t("supplier.jsonTemplate.keyCol");
  const resolvedValCol =
    valueColumnLabel || t("supplier.jsonTemplate.valueCol");
  const resolvedOpCol = opColumnLabel || t("common.fields.operations");
  const resolvedKeyPh = keyPlaceholder || "username";
  const resolvedValPh =
    valuePlaceholder || t("supplier.jsonTemplate.valuePlaceholder");

  return (
    <div className="rounded-lg border border-border bg-card overflow-hidden w-full">
      <div className="flex items-center justify-between gap-2 border-b border-border bg-card px-3 py-1.5">
        <SegmentedControl
          value={mode}
          onChange={handleModeSwitch}
          items={[
            {
              key: "form",
              label: t("supplier.jsonTemplate.formMode"),
              disabled: disabled || !canUseForm,
            },
            {
              key: "raw",
              label: t("supplier.jsonTemplate.rawMode"),
              disabled,
            },
          ]}
        />

        <div className="flex items-center gap-2">
          {!canUseForm && (
            <Tooltip content={t("supplier.jsonTemplate.rawOnlyHint")}>
              <span className="inline-flex items-center gap-1 text-[11px] text-warning-fg">
                <AlertTriangle className="h-3.5 w-3.5" />
                {t("supplier.jsonTemplate.rawOnly")}
              </span>
            </Tooltip>
          )}
          {mode === "raw" && (
            <Button
              type="button"
              variant="link"
              size="xs"
              disabled={disabled}
              onClick={handleFormatJson}
            >
              {t("supplier.jsonTemplate.formatJson")}
            </Button>
          )}
        </div>
      </div>

      {mode === "form" ? (
        <div>
          {(!emptyPlaceholder || formItems.length > 0) && (
            <table className="w-full border-collapse text-xs">
              <thead>
                <tr className="border-b border-border bg-card text-muted-foreground">
                  <th className="h-8 px-2.5 text-left font-medium w-2/5">
                    {resolvedKeyCol}
                  </th>
                  <th className="h-8 px-2.5 text-left font-medium">
                    {resolvedValCol}
                  </th>
                  <th className="h-8 px-2 text-center font-medium w-14">
                    {resolvedOpCol}
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border-subtle">
                {formItems.map((item, idx) => (
                  <tr key={idx}>
                    <td className="p-1.5">
                      <Input
                        value={item.key}
                        disabled={disabled}
                        placeholder={resolvedKeyPh}
                        onChange={(e) => {
                          const next = formItems.map((it, i) =>
                            i === idx ? { ...it, key: e.target.value } : it,
                          );
                          updateItems(next);
                        }}
                      />
                    </td>
                    <td className="p-1.5">
                      <div className="flex items-center gap-1">
                        <Input
                          value={item.value}
                          disabled={disabled}
                          placeholder={resolvedValPh}
                          onChange={(e) => {
                            const next = formItems.map((it, i) =>
                              i === idx ? { ...it, value: e.target.value } : it,
                            );
                            updateItems(next);
                          }}
                        />
                        {variables.length > 0 && (
                          <Popover
                            open={varPopoverIndex === idx}
                            onOpenChange={(o) =>
                              setVarPopoverIndex(o ? idx : null)
                            }
                            align="right"
                            contentClassName="min-w-[140px] p-1"
                            trigger={
                              <Button
                                type="button"
                                variant="outline"
                                size="xs"
                                disabled={disabled}
                                onClick={() =>
                                  setVarPopoverIndex(
                                    varPopoverIndex === idx ? null : idx,
                                  )
                                }
                              >
                                {t("supplier.jsonTemplate.variable")}
                              </Button>
                            }
                          >
                            <div className="flex flex-col gap-0.5">
                              {variables.map((v) => (
                                <button
                                  key={v}
                                  type="button"
                                  onClick={() => {
                                    const next = formItems.map((it, i) =>
                                      i === idx
                                        ? {
                                            ...it,
                                            value: `${it.value || ""}{{ ${v} }}`,
                                          }
                                        : it,
                                    );
                                    updateItems(next);
                                    setVarPopoverIndex(null);
                                  }}
                                  className="rounded-md px-2 py-1 text-left font-mono-tech text-xs text-foreground hover:bg-hover-bg cursor-pointer"
                                >
                                  {`{{ ${v} }}`}
                                </button>
                              ))}
                            </div>
                          </Popover>
                        )}
                      </div>
                    </td>
                    <td className="p-1.5 text-center">
                      <IconButton
                        size="sm"
                        disabled={disabled}
                        aria-label={t("common.actions.delete")}
                        onClick={() => {
                          const next = formItems.filter((_, i) => i !== idx);
                          if (next.length === 0 && !emptyPlaceholder) {
                            next.push({ key: "", value: "" });
                          }
                          updateItems(next);
                        }}
                        className="text-danger-fg hover:text-danger-fg hover:bg-danger-fg/10"
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </IconButton>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          <div className="border-t border-border-subtle px-3 py-1.5">
            <Button
              type="button"
              variant="link"
              size="xs"
              disabled={disabled}
              onClick={() =>
                setFormItems((prev) => [...prev, { key: "", value: "" }])
              }
            >
              <Plus className="h-3.5 w-3.5" />
              {t("supplier.jsonTemplate.addField")}
            </Button>
          </div>
        </div>
      ) : (
        <CodeEditor
          value={value}
          onChange={(next) => {
            validateRaw(next);
            onChange(next);
          }}
          language="json"
          readOnly={disabled}
          minHeight={height}
          placeholder="{}"
          className="border-0 rounded-none"
        />
      )}

      {validationError && (
        <div className="border-t border-danger-fg/30 bg-danger-fg/10 px-3 py-1.5 text-xs text-danger-fg">
          {validationError}
        </div>
      )}
    </div>
  );
}
