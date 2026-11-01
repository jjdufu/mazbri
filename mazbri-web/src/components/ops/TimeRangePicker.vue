<template>
  <div class="ops-time-range-picker">
    <ElSegmented
      v-model="preset"
      :options="presetOptions"
      size="default"
      @change="onPresetChange"
    />
    <ElDatePicker
      v-if="preset === 'custom'"
      v-model="customRange"
      type="datetimerange"
      value-format="x"
      :clearable="false"
      :start-placeholder="t('ops.timeRange.start')"
      :end-placeholder="t('ops.timeRange.end')"
      @change="onCustomChange"
    />
    <ElSelect
      v-model="bookmark"
      class="ops-time-range-picker__bookmark"
      clearable
      :placeholder="t('ops.timeRange.bookmark')"
      @change="onBookmarkChange"
    >
      <ElOption
        v-for="item in bookmarkOptions"
        :key="item.value"
        :label="item.label"
        :value="item.value"
      />
    </ElSelect>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useI18n } from "vue-i18n";

export type TimeRangePreset =
  | "15m"
  | "1h"
  | "6h"
  | "24h"
  | "7d"
  | "30d"
  | "custom";

export interface TimeRangeValue {
  preset: TimeRangePreset;
  start: number;
  end: number;
}

const PRESET_SECONDS: Record<Exclude<TimeRangePreset, "custom">, number> = {
  "15m": 15 * 60,
  "1h": 3600,
  "6h": 6 * 3600,
  "24h": 86400,
  "7d": 7 * 86400,
  "30d": 30 * 86400,
};

const BOOKMARKS: Record<string, number> = {
  loadtest: 2 * 3600,
  event_replay: 6 * 3600,
};

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
const bookmark = ref<string>("");

const presetOptions = computed(() =>
  (["15m", "1h", "6h", "24h", "7d", "30d", "custom"] as TimeRangePreset[]).map(
    (value) => ({
      label: t(`ops.timeRange.presets.${value}`),
      value,
    }),
  ),
);

const bookmarkOptions = computed(() => [
  { label: t("ops.timeRange.bookmarks.loadtest"), value: "loadtest" },
  { label: t("ops.timeRange.bookmarks.eventReplay"), value: "event_replay" },
]);

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

function publish(value: TimeRangeValue) {
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
  bookmark.value = "";
  if (preset.value !== "custom") {
    customRange.value = null;
  }
  publish(buildValue(preset.value));
}

function onCustomChange() {
  if (!customRange.value) return;
  preset.value = "custom";
  publish(buildValue("custom"));
}

function onBookmarkChange(key: string | null) {
  if (!key || !BOOKMARKS[key]) return;
  const end = nowSec();
  const start = end - BOOKMARKS[key];
  preset.value = "custom";
  customRange.value = [start * 1000, end * 1000];
  publish({ preset: "custom", start, end });
}

function hydrateFromQuery() {
  const qPreset = route.query.tr_preset as string | undefined;
  const qStart = Number(route.query.tr_start);
  const qEnd = Number(route.query.tr_end);
  if (
    qPreset &&
    (qPreset in PRESET_SECONDS || qPreset === "custom") &&
    Number.isFinite(qStart) &&
    Number.isFinite(qEnd) &&
    qEnd > qStart
  ) {
    preset.value = qPreset as TimeRangePreset;
    if (qPreset === "custom") {
      customRange.value = [qStart * 1000, qEnd * 1000];
    }
    publish({ preset: preset.value, start: qStart, end: qEnd });
    return;
  }
  if (props.modelValue) {
    preset.value = props.modelValue.preset;
    if (props.modelValue.preset === "custom") {
      customRange.value = [
        props.modelValue.start * 1000,
        props.modelValue.end * 1000,
      ];
    }
    publish(props.modelValue);
    return;
  }
  publish(buildValue(preset.value));
}

onMounted(hydrateFromQuery);

watch(
  () => props.modelValue,
  (v) => {
    if (!v) return;
    if (v.preset !== preset.value) preset.value = v.preset;
    if (v.preset === "custom") {
      customRange.value = [v.start * 1000, v.end * 1000];
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
.ops-time-range-picker__bookmark {
  width: 160px;
}
</style>
