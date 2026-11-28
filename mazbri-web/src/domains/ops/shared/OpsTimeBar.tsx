import React, { useEffect, useMemo, useState } from "react";
import { Check, ChevronDown, RefreshCw } from "lucide-react";
import {
  DateRangePicker,
  IconButton,
  Popover,
  cn,
  type DateRangeValue,
} from "@/components/ui";
import { LanguageEnum } from "@/enums/appEnum";
import { useT } from "@/locales";
import { formatDateTime, parseDisplayWallClockMs } from "@/utils/format";
import {
  PRESET_SECONDS,
  type TimeRangeValue,
} from "@/utils/ops/time-range";

type QuickPreset = "15m" | "1h" | "6h" | "24h";

const PRESETS: readonly QuickPreset[] = ["15m", "1h", "6h", "24h"];

const SHORT_PRESET_LABELS: Record<QuickPreset, string> = {
  "15m": "15m",
  "1h": "1h",
  "6h": "6h",
  "24h": "24h",
};

export interface AutoRefreshControlProps {
  refreshInterval: number;
  onRefreshIntervalChange?: (seconds: number) => void;
  onRefresh?: () => void;
  loading?: boolean;
  showAutoRefresh?: boolean;
  includeFiveMinutes?: boolean;
}

export function AutoRefreshControl({
  refreshInterval,
  onRefreshIntervalChange,
  onRefresh,
  loading = false,
  showAutoRefresh = true,
  includeFiveMinutes = false,
}: AutoRefreshControlProps) {
  const { t, locale } = useT();
  const [refreshMenuOpen, setRefreshMenuOpen] = useState(false);

  const refreshOptions = useMemo(() => {
    const base = [
      { value: 0, short: "", label: t("ops.autoRefresh.off") },
      { value: 30, short: "30s", label: t("ops.autoRefresh.thirtySeconds") },
      { value: 60, short: "1m", label: t("ops.autoRefresh.oneMinute") },
    ];
    if (includeFiveMinutes) {
      base.push({
        value: 300,
        short: "5m",
        label: locale === LanguageEnum.ZH ? "每 5 分钟" : "Every 5m",
      });
    }
    return base;
  }, [t, includeFiveMinutes, locale]);

  const activeRefreshShort =
    refreshOptions.find((o) => o.value === refreshInterval)?.short ?? "";

  const hasAutoRefresh = Boolean(showAutoRefresh && onRefreshIntervalChange);

  if (hasAutoRefresh && onRefresh) {
    return (
      <div className="inline-flex h-8 items-stretch rounded-md border border-control-border bg-control-bg hover:border-border-hover overflow-hidden transition-colors">
        <button
            type="button"
            onClick={onRefresh}
            disabled={loading}
            aria-label={t("common.actions.refresh")}
            className={cn(
              "inline-flex h-full items-center gap-1.5 px-2.5 text-xs transition-colors hover:bg-control-hover active:bg-active-bg disabled:opacity-45 cursor-pointer",
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
              className="inline-flex h-full items-center justify-center border-l border-border-subtle px-1.5 text-muted-foreground hover:bg-control-hover hover:text-foreground transition-colors cursor-pointer"
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
    );
  }

  if (onRefresh) {
    return (
      <IconButton
          size="md"
          onClick={onRefresh}
          disabled={loading}
          aria-label={t("common.actions.refresh")}
          className="border border-control-border bg-control-bg"
        >
          <RefreshCw className={cn("h-3.5 w-3.5", loading && "animate-spin")} />
        </IconButton>
    );
  }

  return null;
}

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

  const isQuickPreset = (PRESETS as readonly string[]).includes(value.preset);

  const pickerValue = useMemo<DateRangeValue>(
    () =>
      !isQuickPreset
        ? {
            start: formatDateTime(value.start * 1000),
            end: formatDateTime(value.end * 1000),
          }
        : { start: "", end: "" },
    [isQuickPreset, value.start, value.end],
  );

  const handlePresetClick = (preset: QuickPreset) => {
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
        "flex flex-wrap items-center gap-1.5 shrink-0",
        className,
      )}
    >
      {extra}

      <div
        role="group"
        aria-label={t("ops.timeRange.start")}
        className="inline-flex h-8 items-center gap-0.5 rounded-md border border-control-border bg-control-bg p-0.5"
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
                  ? "bg-card text-xblue font-semibold shadow-xs"
                  : "font-medium text-muted-foreground hover:text-foreground hover:bg-nav-hover-bg",
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
        clearable={!isQuickPreset}
        align="right"
      />

      <AutoRefreshControl
        refreshInterval={refreshInterval}
        onRefreshIntervalChange={onRefreshIntervalChange}
        onRefresh={onRefresh}
        loading={loading}
        showAutoRefresh={showAutoRefresh}
      />
    </div>
  );
}
