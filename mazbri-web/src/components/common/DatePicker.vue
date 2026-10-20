<template>
  <ElDatePicker
    v-model="modelValue"
    class="art-date-picker"
    v-bind="forwardedAttrs"
    :type="resolvedType"
    :clearable="resolvedClearable"
    :show-footer="resolvedShowFooter"
    :show-confirm="resolvedShowConfirm"
    :show-now="resolvedShowNow"
    :default-time="resolvedDefaultTime"
    :popper-class="mergedPopperClass"
    :cell-class-name="mergedCellClassName"
    :shortcuts="mergedShortcuts"
    @calendar-change="handleCalendarChange"
    @visible-change="handleVisibleChange"
  >
    <template
      v-for="(_, slotName) in slots"
      :key="slotName"
      #[slotName]="slotProps"
    >
      <slot :name="slotName" v-bind="slotProps || {}" />
    </template>
  </ElDatePicker>
</template>

<script setup lang="ts">
import { computed, ref, useAttrs, useSlots, watch } from "vue";
import { ElDatePicker, type DatePickerType } from "element-plus";
import { useI18n } from "vue-i18n";

defineOptions({
  name: "DatePicker",
  inheritAttrs: false,
});

export interface DatePickerShortcut {
  text: string;
  value: () => [Date, Date];
}

const modelValue = defineModel<any>();
const slots = useSlots();
const attrs = useAttrs();
const { t } = useI18n();


const ART_POPPER = "art-date-picker-popper";





const pendingRange = ref<[Date | null, Date | null] | null>(null);







const DATE_ONLY_TYPE_MAP: Record<string, string> = {
  date: "datetime",
  daterange: "datetimerange",
};


function dateOnlyRangeDefaultTime(): [Date, Date] {
  return [
    new Date(2000, 0, 1, 0, 0, 0),
    new Date(2000, 0, 1, 23, 59, 59),
  ];
}
function dateOnlySingleDefaultTime(): Date {
  return new Date(2000, 0, 1, 0, 0, 0);
}

const ATTR_STRIP = new Set([
  "popperClass",
  "popper-class",
  "class",
  "cellClassName",
  "cell-class-name",
  "shortcuts",
  "type",
  "clearable",
  "showFooter",
  "show-footer",
  "showConfirm",
  "show-confirm",
  "showNow",
  "show-now",
  "defaultTime",
  "default-time",
  "onCalendarChange",
  "onCalendar-change",
  "onVisibleChange",
  "onVisible-change",
]);

function coerceBool(value: unknown, fallback: boolean): boolean {
  if (value === undefined || value === null) return fallback;
  if (typeof value === "boolean") return value;

  if (value === "") return true;
  if (value === "false" || value === "0") return false;
  if (value === "true" || value === "1") return true;
  return Boolean(value);
}

function resolveBoolAttr(
  camel: string,
  kebab: string,
  fallback: boolean,
): boolean {
  if (Object.prototype.hasOwnProperty.call(attrs, camel)) {
    return coerceBool(attrs[camel], fallback);
  }
  if (Object.prototype.hasOwnProperty.call(attrs, kebab)) {
    return coerceBool(attrs[kebab], fallback);
  }
  return fallback;
}

const callerType = computed(() => {
  const raw = attrs.type;
  return typeof raw === "string" && raw.trim() ? raw.trim() : "date";
});

const isDateOnlyCallerType = computed(
  () => callerType.value === "date" || callerType.value === "daterange",
);

// 仅类型断言：type 来自调用方透传的 attrs，运行时取值与之前完全相同。
const resolvedType = computed(
  () =>
    (DATE_ONLY_TYPE_MAP[callerType.value] ??
      callerType.value) as DatePickerType,
);


const resolvedClearable = computed(() =>
  resolveBoolAttr("clearable", "clearable", true),
);
const resolvedShowFooter = computed(() =>
  resolveBoolAttr("showFooter", "show-footer", true),
);
const resolvedShowConfirm = computed(() =>
  resolveBoolAttr("showConfirm", "show-confirm", true),
);





const resolvedShowNow = computed(() => {
  if (
    Object.prototype.hasOwnProperty.call(attrs, "showNow") ||
    Object.prototype.hasOwnProperty.call(attrs, "show-now")
  ) {
    return resolveBoolAttr("showNow", "show-now", true);
  }
  return callerType.value !== "date";
});

const resolvedDefaultTime = computed(() => {
  if (Object.prototype.hasOwnProperty.call(attrs, "defaultTime")) {
    return attrs.defaultTime as Date | [Date, Date] | undefined;
  }
  if (Object.prototype.hasOwnProperty.call(attrs, "default-time")) {
    return attrs["default-time"] as Date | [Date, Date] | undefined;
  }
  if (callerType.value === "daterange") return dateOnlyRangeDefaultTime();
  if (callerType.value === "date") return dateOnlySingleDefaultTime();
  return undefined;
});

const forwardedAttrs = computed(() => {
  const rest = { ...attrs } as Record<string, unknown>;
  for (const key of ATTR_STRIP) {
    delete rest[key];
  }
  return rest;
});

const mergedPopperClass = computed(() => {
  const raw = attrs.popperClass ?? attrs["popper-class"];
  const extra = Array.isArray(raw)
    ? raw.filter(Boolean).join(" ")
    : typeof raw === "string"
      ? raw.trim()
      : "";

  const layout = isRangeType() ? " art-date-picker-popper--range-shortcuts" : "";

  const dateOnly = isDateOnlyCallerType.value
    ? " art-date-picker-popper--date-only"
    : "";
  const base = extra ? `${ART_POPPER} ${extra}` : ART_POPPER;
  return `${base}${layout}${dateOnly}`;
});

function parseBound(value: unknown): Date | null {
  if (value == null || value === "") return null;
  if (value instanceof Date) {
    return Number.isNaN(value.getTime()) ? null : value;
  }
  if (typeof value === "number") {
    const d = new Date(value);
    return Number.isNaN(d.getTime()) ? null : d;
  }
  if (typeof value === "string") {
    const trimmed = value.trim();
    if (!trimmed) return null;
    if (/^\d+$/.test(trimmed)) {
      const d = new Date(Number(trimmed));
      return Number.isNaN(d.getTime()) ? null : d;
    }
    const d = new Date(trimmed);
    return Number.isNaN(d.getTime()) ? null : d;
  }
  return null;
}

function dayKey(date: Date): number {
  return Date.UTC(date.getFullYear(), date.getMonth(), date.getDate());
}

function isRangeType(): boolean {
  return callerType.value.includes("range");
}


function isDateTimeRange(): boolean {
  return callerType.value === "datetimerange";
}


function rangeEndingNow(ms: number): [Date, Date] {
  if (isDateTimeRange()) {
    const end = new Date();
    const start = new Date(end.getTime() - ms);
    return [start, end];
  }
  const end = new Date();
  end.setHours(23, 59, 59, 999);
  const start = new Date(end.getTime() - ms);
  start.setHours(0, 0, 0, 0);
  return [start, end];
}

const HOUR = 3600 * 1000;
const DAY = 24 * HOUR;

const defaultRangeShortcuts = computed<DatePickerShortcut[]>(() => [
  {
    text: t("common.datePicker.shortcuts.last1Hour"),
    value: () => rangeEndingNow(1 * HOUR),
  },
  {
    text: t("common.datePicker.shortcuts.last6Hours"),
    value: () => rangeEndingNow(6 * HOUR),
  },
  {
    text: t("common.datePicker.shortcuts.last24Hours"),
    value: () => rangeEndingNow(24 * HOUR),
  },
  {
    text: t("common.datePicker.shortcuts.last7Days"),
    value: () => rangeEndingNow(7 * DAY),
  },
  {
    text: t("common.datePicker.shortcuts.last30Days"),
    value: () => rangeEndingNow(30 * DAY),
  },
]);





const mergedShortcuts = computed(() => {

  if ("shortcuts" in attrs) {
    return attrs.shortcuts as DatePickerShortcut[] | undefined;
  }
  if (!isRangeType()) return undefined;
  return defaultRangeShortcuts.value;
});

function activeRangeBounds(): [Date | null, Date | null] | null {
  if (!isRangeType()) return null;
  if (pendingRange.value) return pendingRange.value;
  const value = modelValue.value;
  if (!Array.isArray(value) || value.length < 1) return null;
  const start = parseBound(value[0]);
  const end = parseBound(value[1]);
  if (!start && !end) return null;
  return [start, end];
}

function artRangeClasses(date: Date): string {
  const bounds = activeRangeBounds();
  if (!bounds) return "";

  const [start, end] = bounds;
  if (!start && !end) return "";

  const key = dayKey(date);
  const startKey = start ? dayKey(start) : null;
  const endKey = end ? dayKey(end) : null;
  const classes: string[] = [];

  if (startKey != null && key === startKey) {
    classes.push("art-range-start");
  }
  if (endKey != null && key === endKey) {
    classes.push("art-range-end");
  }

  if (startKey != null && endKey != null) {
    const lo = Math.min(startKey, endKey);
    const hi = Math.max(startKey, endKey);
    if (key >= lo && key <= hi) {
      classes.push("art-in-range");
    }
  } else if (startKey != null && key === startKey) {

    classes.push("art-in-range");
  }

  return classes.join(" ");
}

function callAttrHandler(name: string, alt: string, payload: unknown) {
  const handler = attrs[name] ?? attrs[alt];
  if (typeof handler === "function") {
    handler(payload);
    return;
  }
  if (Array.isArray(handler)) {
    for (const fn of handler) {
      if (typeof fn === "function") fn(payload);
    }
  }
}

function handleCalendarChange(val: [Date, Date | null]) {
  const start = val?.[0] instanceof Date ? val[0] : null;
  const end = val?.[1] instanceof Date ? val[1] : null;
  pendingRange.value = [start, end];
  callAttrHandler("onCalendarChange", "onCalendar-change", val);
}

function handleVisibleChange(visible: boolean) {
  if (!visible) {
    pendingRange.value = null;
  }
  callAttrHandler("onVisibleChange", "onVisible-change", visible);
}

watch(modelValue, () => {

  pendingRange.value = null;
});

const mergedCellClassName = (date: Date): string => {
  const art = artRangeClasses(date);
  const raw = attrs.cellClassName ?? attrs["cell-class-name"];
  let user = "";
  if (typeof raw === "function") {
    user = String(raw(date) || "");
  } else if (typeof raw === "string") {
    user = raw;
  }
  return [art, user].filter(Boolean).join(" ");
};
</script>
