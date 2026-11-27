<template>
  <div class="ops-time-range-picker">
    <ElSegmented
      v-model="preset"
      :options="presetOptions"
      size="default"
      @change="onPresetChange"
    />
    <DatePicker
      v-if="preset === 'custom'"
      v-model="customRange"
      class="ops-time-range-picker__custom"
      type="datetimerange"
      value-format="YYYY-MM-DD HH:mm:ss"
      :clearable="false"
      :shortcuts="[]"
      :start-placeholder="t('ops.timeRange.start')"
      :end-placeholder="t('ops.timeRange.end')"
      @change="onCustomChange"
    />
    <span class="ops-time-range-picker__tz tabular-nums">UTC+8</span>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useI18n } from "vue-i18n";
import DatePicker from "@/components/common/DatePicker.vue";
import { formatDateTime, parseDisplayWallClockMs } from "@/utils/format";

import {
  PRESET_SECONDS,
  rangeFromQuery,
  sameRange,
  type TimeRangePreset,
  type TimeRangeValue,
} from "@/utils/ops/time-range";

export type { TimeRangePreset, TimeRangeValue };

const props = withDefaults(
  defineProps<{
    modelValue?: TimeRangeValue;
    /** Sync start/end/preset into the current route query. */
    syncQuery?: boolean;
  }>(),
  {
    modelValue: undefined,
    syncQuery: true,
  },
);

const emit = defineEmits<{
  "update:modelValue": [TimeRangeValue];
  change: [TimeRangeValue];
}>();

const { t } = useI18n();
const route = useRoute();
const router = useRouter();

const preset = ref<TimeRangePreset>("1h");
/** East-8 wall-clock strings (`YYYY-MM-DD HH:mm:ss`), converted via explicit `+08:00`. */
const customRange = ref<[string, string] | null>(null);

function toWallClockRange(
  value: Pick<TimeRangeValue, "start" | "end">,
): [string, string] {
  return [formatDateTime(value.start * 1000), formatDateTime(value.end * 1000)];
}

const presetOptions = computed(() =>
  (["15m", "1h", "6h", "24h", "7d", "30d", "custom"] as TimeRangePreset[]).map(
    (value) => ({
      label: t(`ops.timeRange.presets.${value}`),
      value,
    }),
  ),
);

function nowSec() {
  return Math.floor(Date.now() / 1000);
}

function buildValue(nextPreset: TimeRangePreset): TimeRangeValue {
  const end = nowSec();
  if (nextPreset === "custom" && customRange.value) {
    const startMs = parseDisplayWallClockMs(customRange.value[0]);
    const endMs = parseDisplayWallClockMs(customRange.value[1]);
    if (Number.isFinite(startMs) && Number.isFinite(endMs) && endMs > startMs) {
      return {
        preset: "custom",
        start: Math.floor(startMs / 1000),
        end: Math.floor(endMs / 1000),
      };
    }
  }
  const seconds = PRESET_SECONDS[nextPreset as Exclude<TimeRangePreset, "custom">] ?? 3600;
  return { preset: nextPreset, start: end - seconds, end };
}

/** Last range this picker published; route changes equal to it are our own echo. */
let lastPublished: TimeRangeValue | null = null;

function syncQueryParams(value: TimeRangeValue) {
  if (!props.syncQuery) return;
  const nextPreset = value.preset;
  const nextStart = String(value.start);
  const nextEnd = String(value.end);
  if (
    route.query.tr_preset === nextPreset &&
    route.query.tr_start === nextStart &&
    route.query.tr_end === nextEnd
  ) {
    return;
  }
  const query = {
    ...route.query,
    tr_preset: nextPreset,
    tr_start: nextStart,
    tr_end: nextEnd,
  };
  void router.replace({ query });
}

function publish(value: TimeRangeValue) {
  lastPublished = { ...value };
  emit("update:modelValue", value);
  emit("change", value);
  syncQueryParams(value);
}

function onPresetChange() {
  if (preset.value === "custom") {
    if (!customRange.value) {
      const base = lastPublished ?? buildValue("1h");
      customRange.value = toWallClockRange(base);
    }
    return;
  }
  customRange.value = null;
  publish(buildValue(preset.value));
}

function onCustomChange() {
  if (!customRange.value) return;
  preset.value = "custom";
  publish(buildValue("custom"));
}

/** Apply a range from the route query; true when the query carried a valid range. */
function applyQueryRange(emitChange = true): boolean {
  const fromQuery = rangeFromQuery(route.query);
  if (!fromQuery) return false;
  preset.value = fromQuery.preset;
  customRange.value =
    fromQuery.preset === "custom" ? toWallClockRange(fromQuery) : null;
  if (emitChange) {
    publish(fromQuery);
  } else {
    lastPublished = { ...fromQuery };
    if (!sameRange(fromQuery, props.modelValue ?? null)) {
      emit("update:modelValue", fromQuery);
    }
  }
  return true;
}

function hydrateFromQuery() {
  if (applyQueryRange(false)) return;
  if (props.modelValue) {
    preset.value = props.modelValue.preset;
    customRange.value =
      props.modelValue.preset === "custom"
        ? toWallClockRange(props.modelValue)
        : null;
    lastPublished = { ...props.modelValue };
    syncQueryParams(props.modelValue);
    return;
  }
  const initial = buildValue(preset.value);
  lastPublished = { ...initial };
  emit("update:modelValue", initial);
  syncQueryParams(initial);
}

onMounted(hydrateFromQuery);

// In-app hash navigation to the same page (e.g. #/dashboard?tr_preset=custom&...)
// reuses this component, so onMounted does not run again: follow the query here.
watch(
  () => [route.query.tr_preset, route.query.tr_start, route.query.tr_end],
  () => {
    if (!props.syncQuery) return;
    const fromQuery = rangeFromQuery(route.query);
    if (!fromQuery || sameRange(fromQuery, lastPublished)) return;
    applyQueryRange();
  },
);

watch(
  () => props.modelValue,
  (v) => {
    if (!v) return;
    if (v.preset !== preset.value) preset.value = v.preset;
    if (v.preset === "custom") {
      customRange.value = toWallClockRange(v);
    }
  },
);
</script>

<style scoped>
.ops-time-range-picker {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}
.ops-time-range-picker__tz {
  display: inline-flex;
  align-items: center;
  height: 24px;
  padding: 0 7px;
  border-radius: var(--art-form-control-radius-sm, 4px);
  border: 1px solid var(--el-border-color-lighter);
  background: var(--el-fill-color-lighter);
  color: var(--el-text-color-secondary);
  font-size: 11px;
  font-weight: 500;
  line-height: 1;
}
</style>
