import React, { useEffect, useMemo, useState } from "react";
import { Calendar, ChevronLeft, ChevronRight, Clock, X } from "lucide-react";
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
  showPresets?: boolean;
  placeholder?: string;
  clearable?: boolean;
  disabled?: boolean;
  align?: "left" | "right";
  className?: string;
  testId?: string;
}

function pad2(n: number): string {
  return String(n).padStart(2, "0");
}

function parseYmdParts(ymd: string): { y: number; m: number; d: number } | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(ymd.trim());
  if (!m) return null;
  return { y: Number(m[1]), m: Number(m[2]), d: Number(m[3]) };
}

function splitWallClock(raw: string): { ymd: string; hm: string } {
  if (!raw.trim()) return { ymd: "", hm: "" };
  const ms = parseDisplayWallClockMs(raw);
  if (!Number.isFinite(ms)) return { ymd: "", hm: "" };
  const formatted = formatDateTime(ms); // YYYY-MM-DD HH:mm:ss (Asia/Shanghai)
  return { ymd: formatted.slice(0, 10), hm: formatted.slice(11, 16) };
}

function normalizeHm(raw: string, fallback: string): string {
  const cleaned = raw.trim();
  const match = /^(\d{1,2}):(\d{1,2})$/.exec(cleaned);
  if (!match) return fallback;
  const hh = Math.min(23, Math.max(0, Number(match[1])));
  const mm = Math.min(59, Math.max(0, Number(match[2])));
  return `${pad2(hh)}:${pad2(mm)}`;
}

function formatTriggerRange(val: DateRangeValue): string {
  const startMs = parseDisplayWallClockMs(val.start);
  const endMs = parseDisplayWallClockMs(val.end);
  const startText = Number.isFinite(startMs)
    ? formatDateTime(startMs).slice(5, 16)
    : "";
  const endText = Number.isFinite(endMs)
    ? formatDateTime(endMs).slice(5, 16)
    : "";
  if (startText && endText) return `${startText} ~ ${endText}`;
  if (startText) return `${startText} ~`;
  if (endText) return `~ ${endText}`;
  return "";
}

interface CalendarCell {
  ymd: string;
  day: number;
  inMonth: boolean;
}

function buildMonthGrid(year: number, month1: number): CalendarCell[] {
  const firstDay = new Date(Date.UTC(year, month1 - 1, 1));
  // Monday-first offset: Mon=0 .. Sun=6
  const startOffset = (firstDay.getUTCDay() + 6) % 7;
  const daysInMonth = new Date(Date.UTC(year, month1, 0)).getUTCDate();
  const prevMonthDays = new Date(Date.UTC(year, month1 - 1, 0)).getUTCDate();

  const cells: CalendarCell[] = [];
  for (let i = startOffset - 1; i >= 0; i--) {
    const d = prevMonthDays - i;
    const prevDate = new Date(Date.UTC(year, month1 - 2, d));
    cells.push({
      ymd: `${prevDate.getUTCFullYear()}-${pad2(prevDate.getUTCMonth() + 1)}-${pad2(d)}`,
      day: d,
      inMonth: false,
    });
  }
  for (let d = 1; d <= daysInMonth; d++) {
    cells.push({
      ymd: `${year}-${pad2(month1)}-${pad2(d)}`,
      day: d,
      inMonth: true,
    });
  }
  let nextDay = 1;
  while (cells.length < 42) {
    const nextDate = new Date(Date.UTC(year, month1, nextDay));
    cells.push({
      ymd: `${nextDate.getUTCFullYear()}-${pad2(nextDate.getUTCMonth() + 1)}-${pad2(nextDay)}`,
      day: nextDay,
      inMonth: false,
    });
    nextDay++;
  }
  return cells;
}

export function DateRangePicker({
  value,
  onChange,
  presets,
  showPresets = true,
  placeholder,
  clearable = true,
  disabled = false,
  align = "left",
  className,
  testId,
}: DateRangePickerProps) {
  const { tx } = useUiTx();
  const [open, setOpen] = useState(false);

  const todayYmd = formatDisplayDateYmd();
  const todayParts = parseYmdParts(todayYmd) ?? { y: 2026, m: 1, d: 1 };

  const [viewYear, setViewYear] = useState(todayParts.y);
  const [viewMonth, setViewMonth] = useState(todayParts.m);
  const [startYmd, setStartYmd] = useState("");
  const [endYmd, setEndYmd] = useState("");
  const [startHm, setStartHm] = useState("00:00");
  const [endHm, setEndHm] = useState("23:59");
  const [pickingEnd, setPickingEnd] = useState(false);
  const [hoverYmd, setHoverYmd] = useState("");

  useEffect(() => {
    if (!open) return;
    const s = splitWallClock(value.start);
    const e = splitWallClock(value.end);
    setStartYmd(s.ymd);
    setEndYmd(e.ymd);
    setStartHm(s.hm || "00:00");
    setEndHm(e.hm || "23:59");
    setPickingEnd(false);
    setHoverYmd("");
    const anchor = parseYmdParts(e.ymd || s.ymd || todayYmd);
    if (anchor) {
      setViewYear(anchor.y);
      setViewMonth(anchor.m);
    }
  }, [open, value.start, value.end, todayYmd]);

  const effectivePresets = useMemo<readonly DateRangePreset[]>(
    () =>
      presets ?? [
        {
          key: "15m",
          label: tx("近 15 分钟", "Last 15m"),
          durationMs: 15 * 60_000,
        },
        { key: "1h", label: tx("近 1 小时", "Last 1h"), durationMs: 3_600_000 },
        {
          key: "6h",
          label: tx("近 6 小时", "Last 6h"),
          durationMs: 6 * 3_600_000,
        },
        {
          key: "24h",
          label: tx("近 24 小时", "Last 24h"),
          durationMs: 86_400_000,
        },
      ],
    [presets, tx],
  );

  const cells = useMemo(
    () => buildMonthGrid(viewYear, viewMonth),
    [viewYear, viewMonth],
  );

  const weekdays = useMemo(
    () =>
      tx("一,二,三,四,五,六,日", "Mo,Tu,We,Th,Fr,Sa,Su").split(","),
    [tx],
  );

  const summary = formatTriggerRange(value);
  const hasValue = Boolean(value.start || value.end);

  const shiftMonth = (delta: number) => {
    const next = new Date(Date.UTC(viewYear, viewMonth - 1 + delta, 1));
    setViewYear(next.getUTCFullYear());
    setViewMonth(next.getUTCMonth() + 1);
  };

  const handleDayClick = (ymd: string) => {
    if (!pickingEnd || !startYmd) {
      setStartYmd(ymd);
      setEndYmd(ymd);
      setPickingEnd(true);
      return;
    }
    if (ymd < startYmd) {
      setEndYmd(startYmd);
      setStartYmd(ymd);
    } else {
      setEndYmd(ymd);
    }
    setPickingEnd(false);
  };

  const applyPreset = (durationMs: number) => {
    const now = Date.now();
    const startMs = now - durationMs;
    onChange({
      start: formatDateTime(startMs),
      end: formatDateTime(now),
    });
    setOpen(false);
  };

  const handleApply = () => {
    const effStartYmd = startYmd || todayYmd;
    const effEndYmd = endYmd || effStartYmd;
    const normStartHm = normalizeHm(startHm, "00:00");
    const normEndHm = normalizeHm(endHm, "23:59");
    const nextStart = `${effStartYmd} ${normStartHm}:00`;
    const nextEnd = `${effEndYmd} ${normEndHm}:59`;
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
    setStartYmd("");
    setEndYmd("");
    setPickingEnd(false);
    setOpen(false);
  };

  const previewStart =
    pickingEnd && startYmd && hoverYmd
      ? hoverYmd < startYmd
        ? hoverYmd
        : startYmd
      : startYmd;
  const previewEnd =
    pickingEnd && startYmd && hoverYmd
      ? hoverYmd < startYmd
        ? startYmd
        : hoverYmd
      : endYmd;

  return (
    <Popover
      open={open}
      onOpenChange={(nextOpen) => {
        if (!disabled) setOpen(nextOpen);
      }}
      align={align}
      className={className}
      contentClassName="w-[300px] p-3 space-y-3"
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
            "inline-flex h-8 items-center justify-between gap-1.5 rounded-md border border-border bg-card px-2.5 text-xs text-foreground transition-colors hover:bg-hover-bg hover:border-border-hover focus:outline-none focus:ring-2 focus:ring-primary/35 disabled:opacity-50 disabled:pointer-events-none cursor-pointer whitespace-nowrap",
            hasValue && "border-primary/45 bg-primary-soft/40 text-primary",
            open && "ring-2 ring-primary/35 border-primary",
          )}
        >
          <span className="inline-flex items-center gap-1.5 min-w-0 truncate">
            <Calendar className="h-3.5 w-3.5 shrink-0 opacity-75" />
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
              className="inline-flex h-4 w-4 items-center justify-center rounded text-muted-foreground hover:text-foreground hover:bg-active-bg shrink-0"
            >
              <X className="h-3 w-3" />
            </span>
          )}
        </button>
      }
    >
      {showPresets && effectivePresets.length > 0 && (
        <div className="flex flex-wrap gap-1.5 pb-2 border-b border-border">
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
      )}

      {/* Calendar header */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <button
            type="button"
            onClick={() => shiftMonth(-1)}
            aria-label={tx("上个月", "Previous month")}
            className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-hover-bg hover:text-foreground cursor-pointer transition-colors"
          >
            <ChevronLeft className="h-4 w-4" />
          </button>
          <span className="text-xs font-semibold font-mono-tech text-foreground tabular-nums">
            {viewYear}-{pad2(viewMonth)}
          </span>
          <button
            type="button"
            onClick={() => shiftMonth(1)}
            aria-label={tx("下个月", "Next month")}
            className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-hover-bg hover:text-foreground cursor-pointer transition-colors"
          >
            <ChevronRight className="h-4 w-4" />
          </button>
        </div>

        {/* Weekday labels */}
        <div className="grid grid-cols-7 text-center">
          {weekdays.map((wd) => (
            <span
              key={wd}
              className="py-1 text-[11px] font-medium text-muted-foreground select-none"
            >
              {wd}
            </span>
          ))}
        </div>

        {/* Days grid */}
        <div
          className="grid grid-cols-7 gap-y-0.5 text-center"
          onMouseLeave={() => setHoverYmd("")}
        >
          {cells.map((cell) => {
            const isStart = Boolean(previewStart && cell.ymd === previewStart);
            const isEnd = Boolean(previewEnd && cell.ymd === previewEnd);
            const isInRange = Boolean(
              previewStart &&
                previewEnd &&
                cell.ymd > previewStart &&
                cell.ymd < previewEnd,
            );
            const isToday = cell.ymd === todayYmd;

            return (
              <button
                key={cell.ymd}
                type="button"
                onClick={() => handleDayClick(cell.ymd)}
                onMouseEnter={() => {
                  if (pickingEnd) setHoverYmd(cell.ymd);
                }}
                className={cn(
                  "h-7 w-full text-xs font-mono-tech tabular-nums transition-colors cursor-pointer",
                  !cell.inMonth && !isStart && !isEnd && "text-muted-foreground/45",
                  cell.inMonth && !isStart && !isEnd && !isInRange && "text-foreground hover:bg-hover-bg rounded-md",
                  isInRange && "bg-primary-soft text-primary",
                  (isStart || isEnd) && "bg-primary text-primary-foreground font-semibold rounded-md",
                  isToday && !isStart && !isEnd && "ring-1 ring-inset ring-primary/50 rounded-md font-semibold text-primary",
                )}
              >
                {cell.day}
              </button>
            );
          })}
        </div>
      </div>

      {/* Start & End time inputs */}
      <div className="grid grid-cols-2 gap-2 border-t border-border pt-2.5">
        <div className="space-y-1">
          <div className="flex items-center justify-between text-[11px] text-muted-foreground">
            <span>{tx("开始", "Start")}</span>
            <span className="font-mono-tech tabular-nums">
              {startYmd || todayYmd}
            </span>
          </div>
          <div className="flex h-7 items-center gap-1.5 rounded-md border border-input bg-card px-2 focus-within:border-primary focus-within:ring-2 focus-within:ring-primary/25">
            <Clock className="h-3 w-3 shrink-0 text-muted-foreground" />
            <input
              type="text"
              inputMode="numeric"
              value={startHm}
              onChange={(e) => setStartHm(e.target.value)}
              onBlur={() => setStartHm((v) => normalizeHm(v, "00:00"))}
              placeholder="00:00"
              aria-label={tx("开始时间", "Start time")}
              className="w-full bg-transparent text-xs font-mono-tech tabular-nums text-foreground outline-none"
            />
          </div>
        </div>
        <div className="space-y-1">
          <div className="flex items-center justify-between text-[11px] text-muted-foreground">
            <span>{tx("结束", "End")}</span>
            <span className="font-mono-tech tabular-nums">
              {endYmd || startYmd || todayYmd}
            </span>
          </div>
          <div className="flex h-7 items-center gap-1.5 rounded-md border border-input bg-card px-2 focus-within:border-primary focus-within:ring-2 focus-within:ring-primary/25">
            <Clock className="h-3 w-3 shrink-0 text-muted-foreground" />
            <input
              type="text"
              inputMode="numeric"
              value={endHm}
              onChange={(e) => setEndHm(e.target.value)}
              onBlur={() => setEndHm((v) => normalizeHm(v, "23:59"))}
              placeholder="23:59"
              aria-label={tx("结束时间", "End time")}
              className="w-full bg-transparent text-xs font-mono-tech tabular-nums text-foreground outline-none"
            />
          </div>
        </div>
      </div>

      {/* Footer actions */}
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
