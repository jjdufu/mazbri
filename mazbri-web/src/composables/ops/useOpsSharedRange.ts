import {
  inject,
  provide,
  ref,
  type InjectionKey,
  type Ref,
} from "vue";
import {
  defaultTimeRange,
  type TimeRangeValue,
} from "@/utils/ops/time-range";

export interface OpsSharedRange {
  timeRange: Ref<TimeRangeValue>;
  refreshInterval: Ref<number>;
  /** 父级时间窗变更或 AutoRefresh tick 时递增，子面板 watch 后 reload。 */
  reloadTick: Ref<number>;
  bumpReload: () => void;
}

export const OPS_SHARED_RANGE_KEY: InjectionKey<OpsSharedRange> =
  Symbol("opsSharedRange");

/**
 * 合并父页提供共享时间窗 / 刷新间隔；Tab 面板用 useOpsSharedRange 消费。
 * 禁止各 Tab 再各自维护一套 range。
 */
export function provideOpsSharedRange(
  initialPreset: Parameters<typeof defaultTimeRange>[0] = "6h",
  initialRefreshSeconds = 60,
): OpsSharedRange {
  const timeRange = ref<TimeRangeValue>(defaultTimeRange(initialPreset));
  const refreshInterval = ref(initialRefreshSeconds);
  const reloadTick = ref(0);
  function bumpReload() {
    reloadTick.value += 1;
  }
  const api: OpsSharedRange = {
    timeRange,
    refreshInterval,
    reloadTick,
    bumpReload,
  };
  provide(OPS_SHARED_RANGE_KEY, api);
  return api;
}

export function useOpsSharedRange(): OpsSharedRange {
  const ctx = inject(OPS_SHARED_RANGE_KEY, null);
  if (!ctx) {
    throw new Error(
      "useOpsSharedRange() requires provideOpsSharedRange() in an ancestor",
    );
  }
  return ctx;
}
