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
      value-format="x"
      :clearable="false"
      :shortcuts="[]"
      :start-placeholder="t('ops.timeRange.start')"
      :end-placeholder="t('ops.timeRange.end')"
      @change="onCustomChange"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useI18n } from "vue-i18n";
import DatePicker from "@/components/common/DatePicker.vue";

import {
  PRESET_SECONDS,
  pickerRangeOf,
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
/** Epoch milliseconds (ElDatePicker `value-format="x"` works on numbers; numeric strings
 * are parsed as date text and land in the wrong year). The URL keeps Unix seconds. */
const customRange = ref<[number, number] | null>(null);

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
    const start = Math.floor(customRange.value[0] / 1000);
    const endSec = Math.floor(customRange.value[1] / 1000);
    return { preset: "custom", start, end: endSec };
  }
  const seconds = PRESET_SECONDS[nextPreset as Exclude<TimeRangePreset, "custom">] ?? 3600;
  return { preset: nextPreset, start: end - seconds, end };
}

/** Last range this picker published; route changes equal to it are our own echo. */
let lastPublished: TimeRangeValue | null = null;

function publish(value: TimeRangeValue) {
  lastPublished = { ...value };
  emit("update:modelValue", value);
  emit("change", value);
  if (props.syncQuery) {
    const query = {
      ...route.query,
      tr_preset: value.preset,
      tr_start: String(value.start),
      tr_end: String(value.end),
    };
    void router.replace({ query });
  }
}

function onPresetChange() {
  if (preset.value === "custom") {
    if (!customRange.value) {
      const base = lastPublished ?? buildValue("1h");
      customRange.value = pickerRangeOf(base);
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
function applyQueryRange(): boolean {
  const fromQuery = rangeFromQuery(route.query);
  if (!fromQuery) return false;
  preset.value = fromQuery.preset;
  customRange.value =
    fromQuery.preset === "custom" ? pickerRangeOf(fromQuery) : null;
  publish(fromQuery);
  return true;
}

function hydrateFromQuery() {
  if (applyQueryRange()) return;
  if (props.modelValue) {
    preset.value = props.modelValue.preset;
    if (props.modelValue.preset === "custom") {
      customRange.value = pickerRangeOf(props.modelValue);
    }
    publish(props.modelValue);
    return;
  }
  publish(buildValue(preset.value));
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
      customRange.value = pickerRangeOf(v);
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
</style>
