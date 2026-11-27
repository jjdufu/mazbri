import React, { useEffect, useMemo, useState } from "react";
import { Check, ChevronDown, RefreshCw } from "lucide-react";
import {
  DateRangePicker,
  IconButton,
  Popover,
  Tooltip,
  cn,
  type DateRangeValue,
} from "@/components/ui";
import { useT } from "@/locales";
import { formatDateTime, parseDisplayWallClockMs } from "@/utils/format";
import {
  PRESET_SECONDS,
  type TimeRangePreset,
  type TimeRangeValue,
} from "@/utils/ops/time-range";

const PRESETS: readonly Exclude<TimeRangePreset, "custom">[] = [
  "15m",
  "1h",
  "6h",
  "24h",
  "7d",
  "30d",
];

const SHORT_PRESET_LABELS: Record<Exclude<TimeRangePreset, "custom">, string> = {
  "15m": "15m",
  "1h": "1h",
  "6h": "6h",
  "24h": "24h",
  "7d": "7d",
  "30d": "30d",
};

export interface OpsTimeBarProps {
  value: TimeRangeValue;
  onChange: (next: TimeRangeValue) => void;
  refreshInterval?: number;
  onRefreshIntervalChange?: (seconds: number) => void;
  onRefresh?: () => void;
  loading?: boolean;
  showAutoRefresh?: boolean;
  extra?: React.ReactNode;
  className?: string;
}

export function OpsTimeBar({
  value,
  onChange,
  refreshInterval = 0,
  onRefreshIntervalChange,
  onRefresh,
  loading = false,
  showAutoRefresh = true,
  extra,
  className,
}: OpsTimeBarProps) {
  const { t } = useT();
  const [refreshMenuOpen, setRefreshMenuOpen] = useState(false);

  useEffect(() => {
    if (!showAutoRefresh || !refreshInterval || refreshInterval <= 0 || !onRefresh) {
      return;
    }
    const timer = window.setInterval(() => {
      if (typeof document !== "undefined" && document.hidden) return;
      onRefresh();
    }, refreshInterval * 1000);
    return () => window.clearInterval(timer);
  }, [showAutoRefresh, refreshInterval, onRefresh]);

  const pickerValue = useMemo<DateRangeValue>(
    () =>
      value.preset === "custom"
        ? {
            start: formatDateTime(value.start * 1000),
            end: formatDateTime(value.end * 1000),
          }
        : { start: "", end: "" },
    [value],
  );

  const refreshOptions = useMemo(
    () => [
      { value: 0, short: "", label: t("ops.autoRefresh.off") },
      { value: 30, short: "30s", label: t("ops.autoRefresh.thirtySeconds") },
      { value: 60, short: "1m", label: t("ops.autoRefresh.oneMinute") },
    ],
    [t],
  );

  const activeRefreshShort =
    refreshOptions.find((o) => o.value === refreshInterval)?.short ?? "";

  const handlePresetClick = (preset: Exclude<TimeRangePreset, "custom">) => {
    const end = Math.floor(Date.now() / 1000);
    const start = end - PRESET_SECONDS[preset];
    onChange({ preset, start, end });
  };

  const handleCustomChange = (next: DateRangeValue) => {
    if (!next.start || !next.end) {
      handlePresetClick("1h");
      return;
    }
    const sMs = parseDisplayWallClockMs(next.start);
    const eMs = parseDisplayWallClockMs(next.end);
    if (!Number.isFinite(sMs) || !Number.isFinite(eMs) || eMs <= sMs) {
      return;
    }
    onChange({
      preset: "custom",
      start: Math.floor(sMs / 1000),
      end: Math.floor(eMs / 1000),
    });
  };

  const hasAutoRefresh = Boolean(showAutoRefresh && onRefreshIntervalChange);

  return (
    <div
      className={cn(
        "flex flex-wrap items-center gap-2 shrink-0",
        className,
      )}
    >
      {extra}

      <div
        role="group"
        aria-label={t("ops.timeRange.start")}
        className="inline-flex h-8 items-center gap-0.5 rounded-md border border-border bg-card p-0.5"
      >
        {PRESETS.map((preset) => {
          const active = value.preset === preset;
          return (
            <button
              key={preset}
              type="button"
              onClick={() => handlePresetClick(preset)}
              title={t(`ops.timeRange.presets.${preset}`)}
              className={cn(
                "inline-flex h-full items-center justify-center rounded-md px-2 text-xs font-mono-tech transition-colors cursor-pointer whitespace-nowrap",
                active
                  ? "bg-primary-soft text-primary font-semibold"
                  : "font-medium text-muted-foreground hover:text-foreground hover:bg-hover-bg",
              )}
            >
              {SHORT_PRESET_LABELS[preset]}
            </button>
          );
        })}
      </div>

      <DateRangePicker
        value={pickerValue}
        onChange={handleCustomChange}
        showPresets={false}
        placeholder={t("ops.timeRange.presets.custom")}
        clearable={value.preset === "custom"}
        align="right"
      />

      {hasAutoRefresh && onRefresh ? (
        <div className="inline-flex h-8 items-stretch rounded-md border border-border bg-card overflow-hidden">
          <Tooltip content={t("ops.autoRefresh.refreshNow")}>
            <button
              type="button"
              onClick={onRefresh}
              disabled={loading}
              aria-label={t("ops.autoRefresh.refreshNow")}
              className={cn(
                "inline-flex h-full items-center gap-1.5 px-2.5 text-xs transition-colors hover:bg-hover-bg active:bg-active-bg disabled:opacity-45 cursor-pointer",
                activeRefreshShort
                  ? "text-primary font-medium"
                  : "text-muted-foreground hover:text-foreground",
              )}
            >
              <RefreshCw
                className={cn("h-3.5 w-3.5 shrink-0", loading && "animate-spin")}
              />
              {activeRefreshShort && (
                <span className="font-mono-tech text-[11px] tabular-nums">
                  {activeRefreshShort}
                </span>
              )}
            </button>
          </Tooltip>
          <Popover
            open={refreshMenuOpen}
            onOpenChange={setRefreshMenuOpen}
            align="right"
            contentClassName="w-32 p-1"
            trigger={
              <button
                type="button"
                onClick={() => setRefreshMenuOpen((v) => !v)}
                aria-label={t("ops.autoRefresh.label")}
                className="inline-flex h-full items-center justify-center border-l border-border px-1.5 text-muted-foreground hover:bg-hover-bg hover:text-foreground transition-colors cursor-pointer"
              >
                <ChevronDown className="h-3.5 w-3.5" />
              </button>
            }
          >
            <div className="flex flex-col gap-0.5">
              {refreshOptions.map((opt) => {
                const active = opt.value === refreshInterval;
                return (
                  <button
                    key={opt.value}
                    type="button"
                    onClick={() => {
                      onRefreshIntervalChange?.(opt.value);
                      setRefreshMenuOpen(false);
                    }}
                    className={cn(
                      "flex w-full h-7 items-center justify-between gap-2 rounded-md px-2 text-xs transition-colors cursor-pointer text-left",
                      active
                        ? "bg-primary-soft text-primary font-medium"
                        : "text-foreground hover:bg-hover-bg",
                    )}
                  >
                    <span>{opt.label}</span>
                    {active && <Check className="h-3.5 w-3.5 shrink-0" />}
                  </button>
                );
              })}
            </div>
          </Popover>
        </div>
      ) : onRefresh ? (
        <Tooltip content={t("ops.autoRefresh.refreshNow")}>
          <IconButton
            size="md"
            onClick={onRefresh}
            disabled={loading}
            aria-label={t("ops.autoRefresh.refreshNow")}
            className="border border-border bg-card"
          >
            <RefreshCw className={cn("h-3.5 w-3.5", loading && "animate-spin")} />
          </IconButton>
        </Tooltip>
      ) : null}
    </div>
  );
}
