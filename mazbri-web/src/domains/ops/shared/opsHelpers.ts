import { useCallback, useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router";
import { useAuthStore } from "@/stores/auth";
import type { OpsRange } from "@/types/api/billing-ops";
import type { RouteLocationRaw } from "@/types/router";
import { isAdminRole, isBizRole, isViewerRole } from "@/utils/ops/roles";
import { resolveOpsTab, type OpsTabDef } from "@/utils/ops/tabs";
import {
  PRESET_SECONDS,
  defaultTimeRange,
  rangeFromQuery,
  sameRange,
  type TimeRangePreset,
  type TimeRangeValue,
} from "@/utils/ops/time-range";

export const STEP_CANDIDATES = [15, 60, 300, 3600] as const;

export function resolveStep(rangeSeconds: number): number {
  if (!Number.isFinite(rangeSeconds) || rangeSeconds <= 0) {
    return STEP_CANDIDATES[0];
  }
  const raw = rangeSeconds / 300;
  for (const candidate of STEP_CANDIDATES) {
    if (raw <= candidate) {
      return candidate;
    }
  }
  return STEP_CANDIDATES[STEP_CANDIDATES.length - 1];
}

export function toOpsRange(range: TimeRangeValue): OpsRange {
  if (range.preset !== "custom") {
    return range.preset;
  }
  const secs = Math.max(1, range.end - range.start);
  if (secs <= 900) return "15m";
  if (secs <= 3600) return "1h";
  if (secs <= 6 * 3600) return "6h";
  if (secs <= 86400) return "24h";
  if (secs <= 7 * 86400) return "7d";
  return "30d";
}

export function toLocationHref(target: RouteLocationRaw | null | undefined): string {
  if (!target) return "";
  if (typeof target === "string") return target;
  const path = target.path ?? "";
  const query = target.query;
  if (!query) return path;
  const params = new URLSearchParams();
  for (const [k, v] of Object.entries(query)) {
    if (v === undefined || v === null) continue;
    if (Array.isArray(v)) {
      for (const item of v) {
        if (item !== null && item !== undefined) params.append(k, String(item));
      }
    } else {
      params.set(k, String(v));
    }
  }
  const qs = params.toString();
  return qs ? `${path}?${qs}` : path;
}

export function useOpsRole() {
  const role = useAuthStore((s) => s.info.role);
  return useMemo(
    () => ({
      role,
      isAdmin: isAdminRole(role),
      isBiz: isBizRole(role),
      isViewer: isViewerRole(role),
    }),
    [role],
  );
}

export function useOpsTabUrl<T extends string>(
  tabs: readonly OpsTabDef<T>[],
  preferredDefault: T,
) {
  const { role } = useOpsRole();
  const [searchParams, setSearchParams] = useSearchParams();
  const queryTab = searchParams.get("tab") ?? undefined;

  const resolved = useMemo(
    () =>
      resolveOpsTab({
        role,
        tabs,
        queryTab,
        preferredDefault,
      }),
    [role, tabs, queryTab, preferredDefault],
  );

  const activeTab = (resolved.activeTab ?? preferredDefault) as T;

  useEffect(() => {
    if (resolved.activeTab && queryTab && resolved.fellBack) {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          next.set("tab", resolved.activeTab!);
          return next;
        },
        { replace: true },
      );
    }
  }, [resolved.activeTab, resolved.fellBack, queryTab, setSearchParams]);

  const setActiveTab = useCallback(
    (nextTab: T) => {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          next.set("tab", nextTab);
          return next;
        },
        { replace: true },
      );
    },
    [setSearchParams],
  );

  return {
    visibleTabs: resolved.visibleTabs,
    activeTab,
    setActiveTab,
  };
}

export function useOpsTimeRangeUrl(
  defaultPreset: Exclude<TimeRangePreset, "custom"> = "1h",
) {
  const [searchParams, setSearchParams] = useSearchParams();

  const queryRange = useMemo(() => {
    const obj: Record<string, string | null> = {
      tr_preset: searchParams.get("tr_preset"),
      tr_start: searchParams.get("tr_start"),
      tr_end: searchParams.get("tr_end"),
    };
    return rangeFromQuery(obj);
  }, [searchParams]);

  const [timeRange, setTimeRangeState] = useState<TimeRangeValue>(
    () => queryRange ?? defaultTimeRange(defaultPreset),
  );

  useEffect(() => {
    if (queryRange && !sameRange(queryRange, timeRange)) {
      setTimeRangeState(queryRange);
    }
  }, [queryRange, timeRange]);

  const setTimeRange = useCallback(
    (next: TimeRangeValue) => {
      setTimeRangeState(next);
      setSearchParams(
        (prev) => {
          const params = new URLSearchParams(prev);
          params.set("tr_preset", next.preset);
          params.set("tr_start", String(Math.floor(next.start)));
          params.set("tr_end", String(Math.floor(next.end)));
          return params;
        },
        { replace: true },
      );
    },
    [setSearchParams],
  );

  const selectPreset = useCallback(
    (preset: Exclude<TimeRangePreset, "custom">) => {
      const end = Math.floor(Date.now() / 1000);
      const start = end - PRESET_SECONDS[preset];
      setTimeRange({ preset, start, end });
    },
    [setTimeRange],
  );

  return {
    timeRange,
    setTimeRange,
    selectPreset,
  };
}
