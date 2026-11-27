import React, { useEffect, useMemo, useState } from "react";
import { Calendar, X } from "lucide-react";
import {
  formatDateTime,
  formatDisplayDateYmd,
  parseDisplayWallClockMs,
} from "@/utils/format";
import { cn } from "./cn";
import { ActionChip, Button, Popover } from "./primitives";
import { useUiTx } from "./tx";

export interface DateRangeValue {
  start: string;
  end: string;
}

export interface DateRangePreset {
  readonly key: string;
  readonly label: string;
  readonly durationMs: number;
}

export interface DateRangePickerProps {
  value: DateRangeValue;
  onChange: (next: DateRangeValue) => void;
  presets?: readonly DateRangePreset[];
  placeholder?: string;
  clearable?: boolean;
  disabled?: boolean;
  align?: "left" | "right";
  className?: string;
  testId?: string;
}

function toDatetimeLocalValue(raw: string): string {
  if (!raw.trim()) return "";
  const ms = parseDisplayWallClockMs(raw);
  if (!Number.isFinite(ms)) return "";
  const formatted = formatDateTime(ms);
  return formatted.slice(0, 16).replace(" ", "T");
}

function fromDatetimeLocalValue(localStr: string, isEnd: boolean): string {
  const trimmed = localStr.trim();
  if (!trimmed) return "";
  const normalized = trimmed.replace("T", " ");
  if (/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/.test(normalized)) {
    return `${normalized}:${isEnd ? "59" : "00"}`;
  }
  const ms = parseDisplayWallClockMs(normalized);
  if (!Number.isFinite(ms)) return "";
  return formatDateTime(ms);
}

function formatTriggerRange(val: DateRangeValue): string {
  const startMs = parseDisplayWallClockMs(val.start);
  const endMs = parseDisplayWallClockMs(val.end);
  const startText = Number.isFinite(startMs)
    ? formatDateTime(startMs).slice(0, 16)
    : "";
  const endText = Number.isFinite(endMs)
    ? formatDateTime(endMs).slice(0, 16)
    : "";
  if (startText && endText) return `${startText} ~ ${endText}`;
  if (startText) return `${startText} ~`;
  if (endText) return `~ ${endText}`;
  return "";
}

export function DateRangePicker({
  value,
  onChange,
  presets,
  placeholder,
  clearable = true,
  disabled = false,
  align = "left",
  className,
  testId,
}: DateRangePickerProps) {
  const { tx } = useUiTx();
  const [open, setOpen] = useState(false);
  const [draftStart, setDraftStart] = useState("");
  const [draftEnd, setDraftEnd] = useState("");

  useEffect(() => {
    if (open) {
      setDraftStart(toDatetimeLocalValue(value.start));
      setDraftEnd(toDatetimeLocalValue(value.end));
    }
  }, [open, value.start, value.end]);

  const effectivePresets = useMemo<readonly DateRangePreset[]>(
    () =>
      presets ?? [
        { key: "1h", label: tx("近 1 小时", "Last 1h"), durationMs: 3_600_000 },
        {
          key: "24h",
          label: tx("近 24 小时", "Last 24h"),
          durationMs: 86_400_000,
        },
        {
          key: "7d",
          label: tx("近 7 天", "Last 7d"),
          durationMs: 7 * 86_400_000,
        },
        {
          key: "30d",
          label: tx("近 30 天", "Last 30d"),
          durationMs: 30 * 86_400_000,
        },
      ],
    [presets, tx],
  );

  const summary = formatTriggerRange(value);
  const hasValue = Boolean(value.start || value.end);

  const applyPreset = (durationMs: number) => {
    const now = Date.now();
    const startMs = now - durationMs;
    const next: DateRangeValue = {
      start: formatDateTime(startMs),
      end: formatDateTime(now),
    };
    onChange(next);
    setOpen(false);
  };

  const handleApply = () => {
    const nextStart = fromDatetimeLocalValue(draftStart, false);
    const nextEnd = fromDatetimeLocalValue(draftEnd, true);
    const sMs = parseDisplayWallClockMs(nextStart);
    const eMs = parseDisplayWallClockMs(nextEnd);
    if (Number.isFinite(sMs) && Number.isFinite(eMs) && sMs > eMs) {
      onChange({ start: nextEnd, end: nextStart });
    } else {
      onChange({ start: nextStart, end: nextEnd });
    }
    setOpen(false);
  };

  const handleClear = (e?: React.MouseEvent) => {
    e?.stopPropagation();
    onChange({ start: "", end: "" });
    setDraftStart("");
    setDraftEnd("");
    setOpen(false);
  };

  const todayYmd = formatDisplayDateYmd();

  return (
    <Popover
      open={open}
      onOpenChange={(nextOpen) => {
        if (!disabled) setOpen(nextOpen);
      }}
      align={align}
      className={className}
      contentClassName="w-[340px] p-3 space-y-3"
      contentTestId={testId ? `${testId}-popover` : undefined}
      trigger={
        <button
          type="button"
          disabled={disabled}
          data-testid={testId}
          onClick={() => {
            if (!disabled) setOpen((v) => !v);
          }}
          className={cn(
            "inline-flex h-8 items-center justify-between gap-2 rounded-md border border-input bg-card px-2.5 text-xs text-foreground transition-all duration-150 hover:bg-hover-bg hover:border-border-hover focus:outline-none focus:ring-2 focus:ring-primary/35 disabled:opacity-50 disabled:pointer-events-none cursor-pointer min-w-[220px]",
            open && "ring-2 ring-primary/35 border-primary",
          )}
        >
          <span className="inline-flex items-center gap-1.5 min-w-0 truncate">
            <Calendar className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
            {summary ? (
              <span className="font-mono-tech tabular-nums truncate">
                {summary}
              </span>
            ) : (
              <span className="text-muted-foreground truncate">
                {placeholder ?? tx("时间范围", "Date range")}
              </span>
            )}
          </span>
          {clearable && hasValue && !disabled && (
            <span
              role="button"
              tabIndex={-1}
              aria-label={tx("清空", "Clear")}
              onClick={handleClear}
              className="inline-flex h-4 w-4 items-center justify-center rounded-md text-muted-foreground hover:text-foreground hover:bg-active-bg shrink-0"
            >
              <X className="h-3 w-3" />
            </span>
          )}
        </button>
      }
    >
      <div className="space-y-1.5">
        <div className="flex items-center justify-between text-[11px] font-medium text-muted-foreground">
          <span>{tx("快捷区间", "Presets")}</span>
          <span className="font-mono-tech">UTC+8 · {todayYmd}</span>
        </div>
        <div className="flex flex-wrap gap-1.5">
          {effectivePresets.map((preset) => (
            <ActionChip
              key={preset.key}
              size="xs"
              onClick={() => applyPreset(preset.durationMs)}
            >
              {preset.label}
            </ActionChip>
          ))}
        </div>
      </div>

      <div className="space-y-2 border-t border-border pt-2.5">
        <div className="space-y-1">
          <label className="text-[11px] font-medium text-muted-foreground">
            {tx("开始时间", "Start")}
          </label>
          <input
            type="datetime-local"
            value={draftStart}
            onChange={(e) => setDraftStart(e.target.value)}
            className="flex h-8 w-full rounded-md border border-input bg-card px-2.5 text-xs font-mono-tech text-foreground hover:border-border-hover focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary/30"
          />
        </div>
        <div className="space-y-1">
          <label className="text-[11px] font-medium text-muted-foreground">
            {tx("结束时间", "End")}
          </label>
          <input
            type="datetime-local"
            value={draftEnd}
            onChange={(e) => setDraftEnd(e.target.value)}
            className="flex h-8 w-full rounded-md border border-input bg-card px-2.5 text-xs font-mono-tech text-foreground hover:border-border-hover focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary/30"
          />
        </div>
      </div>

      <div className="flex items-center justify-end gap-2 border-t border-border pt-2.5">
        {clearable && (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => handleClear()}
          >
            {tx("清空", "Clear")}
          </Button>
        )}
        <Button
          type="button"
          variant="primary"
          size="sm"
          onClick={handleApply}
        >
          {tx("应用", "Apply")}
        </Button>
      </div>
    </Popover>
  );
}
