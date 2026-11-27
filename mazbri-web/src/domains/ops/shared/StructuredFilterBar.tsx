import React, { useMemo, useState } from "react";
import { Filter, Plus, Search, X } from "lucide-react";
import {
  Button,
  Input,
  Popover,
  Select,
  cn,
  type SelectOptionItem,
} from "@/components/ui";
import { useT } from "@/locales";
import type {
  FilterFieldDef,
  FilterOp,
  ListFilter,
  ResourceFilterCatalog,
} from "@/types/list-query";
import { formatFilterChipText } from "@/utils/list-query/serialize";
import { showWarningToast } from "@/utils/ui/message-guard";

export interface StructuredFilterBarProps {
  q: string;
  onQChange: (next: string) => void;
  filters: readonly ListFilter[];
  onFiltersChange: (next: ListFilter[]) => void;
  catalog: ResourceFilterCatalog;
  placeholder?: string;
  onSearch: () => void;
  className?: string;
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
}: StructuredFilterBarProps) {
  const { t } = useT();
  const [open, setOpen] = useState(false);

  const fields = catalog.fields;
  const [selectedField, setSelectedField] = useState<string>(
    () => fields[0]?.field ?? "",
  );

  const fieldDef: FilterFieldDef | undefined = useMemo(
    () => fields.find((f) => f.field === selectedField) ?? fields[0],
    [fields, selectedField],
  );

  const [selectedOp, setSelectedOp] = useState<FilterOp>(
    () => fieldDef?.ops[0] ?? "eq",
  );
  const [rawVal, setRawVal] = useState("");
  const [rawValEnd, setRawValEnd] = useState("");

  const fieldOptions = useMemo<SelectOptionItem[]>(
    () =>
      fields.map((f) => ({
        value: f.field,
        label: t(f.label_key),
        text: t(f.label_key),
      })),
    [fields, t],
  );

  const opOptions = useMemo<SelectOptionItem[]>(
    () =>
      (fieldDef?.ops ?? ["eq"]).map((op) => ({
        value: op,
        label: t(`listFilters.ops.${op}`),
        text: t(`listFilters.ops.${op}`),
      })),
    [fieldDef, t],
  );

  const handleFieldChange = (nextField: string) => {
    setSelectedField(nextField);
    const def = fields.find((f) => f.field === nextField);
    if (def && !def.ops.includes(selectedOp)) {
      setSelectedOp(def.ops[0] ?? "eq");
    }
    setRawVal("");
    setRawValEnd("");
  };

  const parseValueForField = (
    def: FilterFieldDef,
    raw: string,
  ): string | number | null => {
    const trimmed = raw.trim();
    if (!trimmed) return null;
    if (def.value_type === "int" || def.value_type === "uint") {
      const cleaned = def.unit === "ms" ? trimmed.replace(/ms$/i, "").trim() : trimmed;
      const n = Number(cleaned);
      if (!Number.isFinite(n)) return null;
      if (def.value_type === "uint" && n < 0) return null;
      return Math.trunc(n);
    }
    return trimmed;
  };

  const handleAddFilter = () => {
    if (!fieldDef) return;
    const maxFilters = catalog.max_filters ?? 16;
    if (filters.length >= maxFilters) {
      showWarningToast(t("listFilters.tooMany", { max: maxFilters }));
      return;
    }

    let nextFilter: ListFilter;
    if (selectedOp === "is_null" || selectedOp === "is_not_null") {
      nextFilter = { field: fieldDef.field, op: selectedOp };
    } else if (selectedOp === "between") {
      const a = parseValueForField(fieldDef, rawVal);
      const b = parseValueForField(fieldDef, rawValEnd);
      if (a === null || b === null) {
        showWarningToast(t("listFilters.invalidValue"));
        return;
      }
      const pair: [number, number] | [string, string] =
        typeof a === "number" && typeof b === "number"
          ? [a, b]
          : [String(a), String(b)];
      nextFilter = { field: fieldDef.field, op: "between", value: pair };
    } else {
      const val = parseValueForField(fieldDef, rawVal);
      if (val === null) {
        showWarningToast(t("listFilters.invalidValue"));
        return;
      }
      nextFilter = { field: fieldDef.field, op: selectedOp, value: val };
    }

    onFiltersChange([...filters, nextFilter]);
    setRawVal("");
    setRawValEnd("");
    setOpen(false);
    onSearch();
  };

  const handleRemoveFilter = (idx: number) => {
    const next = filters.filter((_, i) => i !== idx);
    onFiltersChange(next);
    onSearch();
  };

  const valuePlaceholder = useMemo(() => {
    if (!fieldDef) return "";
    if (fieldDef.unit === "ms") return t("listFilters.valueMs");
    if (fieldDef.value_type === "datetime") return t("listFilters.valueDatetime");
    return t("listFilters.value");
  }, [fieldDef, t]);

  const fieldMap = useMemo(() => {
    const map = new Map<string, FilterFieldDef>();
    for (const f of fields) map.set(f.field, f);
    return map;
  }, [fields]);

  const isEnumField =
    fieldDef?.value_type === "enum" &&
    Array.isArray(fieldDef.enum_values) &&
    fieldDef.enum_values.length > 0;

  return (
    <div className={cn("flex flex-wrap items-center gap-2 min-w-0", className)}>
      <div className="relative flex items-center w-60 max-w-full">
        <Search className="pointer-events-none absolute left-2.5 h-3.5 w-3.5 text-muted-foreground" />
        <Input
          value={q}
          onChange={(e) => onQChange(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              onSearch();
            }
          }}
          placeholder={placeholder ?? t("common.search")}
          className="pl-8 pr-7"
        />
        {q && (
          <button
            type="button"
            onClick={() => {
              onQChange("");
              onSearch();
            }}
            aria-label={t("common.reset")}
            className="absolute right-1.5 inline-flex h-5 w-5 items-center justify-center rounded-md text-muted-foreground hover:text-foreground hover:bg-hover-bg cursor-pointer"
          >
            <X className="h-3 w-3" />
          </button>
        )}
      </div>

      <Popover
        open={open}
        onOpenChange={setOpen}
        align="left"
        contentClassName="w-[360px] p-3 space-y-3"
        trigger={
          <Button
            type="button"
            variant="outline"
            size="md"
            active={filters.length > 0}
            onClick={() => setOpen((v) => !v)}
          >
            <Filter className="h-3.5 w-3.5" />
            <span>{t("listFilters.tooltip")}</span>
            {filters.length > 0 && (
              <span className="rounded-full bg-primary-soft px-1.5 text-[10px] font-mono-tech text-primary">
                {filters.length}
              </span>
            )}
          </Button>
        }
      >
        <div className="space-y-2.5">
          <div className="grid grid-cols-12 gap-2">
            <div className="col-span-7">
              <label className="mb-1 block text-[11px] font-medium text-muted-foreground">
                {t("listFilters.field")}
              </label>
              <Select
                value={fieldDef?.field ?? ""}
                options={fieldOptions}
                onChange={(e) => handleFieldChange(e.target.value)}
                className="w-full"
              />
            </div>
            <div className="col-span-5">
              <label className="mb-1 block text-[11px] font-medium text-muted-foreground">
                {t("listFilters.op")}
              </label>
              <Select
                value={selectedOp}
                options={opOptions}
                onChange={(e) => setSelectedOp(e.target.value as FilterOp)}
                className="w-full"
              />
            </div>
          </div>

          {selectedOp !== "is_null" && selectedOp !== "is_not_null" && (
            <div>
              <label className="mb-1 block text-[11px] font-medium text-muted-foreground">
                {t("listFilters.value")}
              </label>
              {selectedOp === "between" ? (
                <div className="grid grid-cols-2 gap-2">
                  <Input
                    value={rawVal}
                    onChange={(e) => setRawVal(e.target.value)}
                    placeholder={t("listFilters.from")}
                  />
                  <Input
                    value={rawValEnd}
                    onChange={(e) => setRawValEnd(e.target.value)}
                    placeholder={t("listFilters.to")}
                  />
                </div>
              ) : isEnumField ? (
                <Select
                  value={rawVal}
                  options={(fieldDef.enum_values ?? []).map((val) => ({
                    value: val,
                    label: val,
                    text: val,
                  }))}
                  placeholder={t("listFilters.value")}
                  onChange={(e) => setRawVal(e.target.value)}
                  className="w-full"
                />
              ) : (
                <Input
                  value={rawVal}
                  onChange={(e) => setRawVal(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      handleAddFilter();
                    }
                  }}
                  placeholder={valuePlaceholder}
                />
              )}
            </div>
          )}

          <div className="flex items-center justify-between pt-1">
            {filters.length > 0 ? (
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={() => {
                  onFiltersChange([]);
                  setOpen(false);
                  onSearch();
                }}
              >
                {t("listFilters.clearAll")}
              </Button>
            ) : (
              <span />
            )}
            <Button
              type="button"
              variant="primary"
              size="sm"
              onClick={handleAddFilter}
            >
              <Plus className="h-3.5 w-3.5" />
              {t("listFilters.add")}
            </Button>
          </div>
        </div>
      </Popover>

      {filters.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5">
          {filters.map((f, idx) => {
            const def = fieldMap.get(f.field);
            const label = formatFilterChipText({
              fieldLabel: def ? t(def.label_key) : f.field,
              opLabel: t(`listFilters.ops.${f.op}`),
              value: f.value,
              unit: def?.unit,
            });
            return (
              <span
                key={`${f.field}-${f.op}-${idx}`}
                className="inline-flex h-7 items-center gap-1 rounded-md border border-primary/35 bg-primary-soft px-2 text-xs text-primary"
              >
                <span className="max-w-[200px] truncate font-mono-tech">
                  {label}
                </span>
                <button
                  type="button"
                  onClick={() => handleRemoveFilter(idx)}
                  aria-label={t("listFilters.remove")}
                  className="inline-flex h-4 w-4 items-center justify-center rounded-md hover:bg-primary-soft-hover cursor-pointer"
                >
                  <X className="h-3 w-3" />
                </button>
              </span>
            );
          })}
        </div>
      )}
    </div>
  );
}
