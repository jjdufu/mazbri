import React, { useEffect, useMemo } from "react";
import { RefreshCw } from "lucide-react";
import {
  DateRangePicker,
  IconButton,
  Select,
  Tooltip,
  cn,
  type DateRangeValue,
  type SelectOptionItem,
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

  const pickerPresets = useMemo(
    () =>
      PRESETS.map((key) => ({
        key,
        label: t(`ops.timeRange.presets.${key}`),
        durationMs: PRESET_SECONDS[key] * 1000,
      })),
    [t],
  );

  const refreshOptions = useMemo<SelectOptionItem[]>(
    () => [
      {
        value: "0",
        label: t("ops.autoRefresh.off"),
        text: t("ops.autoRefresh.off"),
      },
      {
        value: "30",
        label: t("ops.autoRefresh.thirtySeconds"),
        text: t("ops.autoRefresh.thirtySeconds"),
      },
      {
        value: "60",
        label: t("ops.autoRefresh.oneMinute"),
        text: t("ops.autoRefresh.oneMinute"),
      },
    ],
    [t],
  );

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

  return (
    <div
      className={cn(
        "flex flex-wrap items-center gap-2",
        className,
      )}
    >
      {extra}

      <div
        role="group"
        aria-label={t("ops.timeRange.start")}
        className="inline-flex h-8 items-center gap-0.5 rounded-md border border-border bg-muted/60 p-0.5"
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
                  ? "bg-primary-soft text-primary font-semibold shadow-2xs"
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
        presets={pickerPresets}
        placeholder={t("ops.timeRange.presets.custom")}
        clearable={value.preset === "custom"}
        align="right"
      />

      <span className="inline-flex h-8 items-center rounded-md border border-border bg-muted/40 px-2 text-[11px] font-mono-tech text-muted-foreground select-none">
        UTC+8
      </span>

      {showAutoRefresh && onRefreshIntervalChange && (
        <Select
          value={String(refreshInterval)}
          options={refreshOptions}
          onChange={(e) => {
            const secs = Number(e.target.value);
            if (Number.isFinite(secs) && secs >= 0) {
              onRefreshIntervalChange(secs);
            }
          }}
          aria-label={t("ops.autoRefresh.label")}
          className="w-32"
        />
      )}

      {onRefresh && (
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
      )}
    </div>
  );
}
