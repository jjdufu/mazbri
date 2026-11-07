<template>
  <MetricSummaryCard
    class="ops-stat-card"
    :class="{ 'ops-stat-card--interactive': interactive }"
    :label="label"
    :icon="icon"
    :icon-color="iconColor"
    :icon-background="iconBackground"
    :loading="loading"
    @click="onCardClick"
  >
    <div class="ops-stat-card__main">
      <span
        class="ops-stat-card__value"
        :class="{ 'ops-stat-card__value--empty': !hasValue }"
      >{{ displayValue }}</span>
      <span v-if="unit && hasValue" class="ops-stat-card__unit">{{
        unit
      }}</span>
    </div>
    <span v-if="hint" class="ops-stat-card__hint" :title="hint">{{
      hint
    }}</span>
  </MetricSummaryCard>
</template>

<script setup lang="ts">
import { computed } from "vue";
import MetricSummaryCard from "@/components/common/metric-summary-card/index.vue";

const emit = defineEmits<{ click: [MouseEvent] }>();

const props = withDefaults(
  defineProps<{
    label: string;
    value?: number | string | null;
    unit?: string;
    hint?: string;
    icon?: string;
    iconColor?: string;
    iconBackground?: string;
    loading?: boolean;
    /** 可下钻时显示指针；无交互时不冒充可点。 */
    interactive?: boolean;
    digits?: number;
    /** 值为空时展示文案（默认 N/A） */
    emptyText?: string;
  }>(),
  {
    value: null,
    unit: "",
    hint: "",
    icon: "ri:bar-chart-line",
    iconColor: "#409eff",
    iconBackground: "rgba(64, 158, 255, 0.1)",
    loading: false,
    interactive: false,
    digits: 2,
    emptyText: "N/A",
  },
);

/** 空值不显示单位（避免「无样本 ms」之类的拼接）。 */
const hasValue = computed(
  () => props.value !== null && props.value !== undefined && props.value !== "",
);

const displayValue = computed(() => {
  if (!hasValue.value) {
    return props.emptyText;
  }
  if (typeof props.value === "number") {
    return new Intl.NumberFormat(undefined, {
      maximumFractionDigits: props.digits,
    }).format(props.value);
  }
  return String(props.value);
});

function onCardClick(event: MouseEvent) {
  if (!props.interactive) return;
  emit("click", event);
}
</script>

<style scoped>
.ops-stat-card {
  height: 100%;
  min-height: var(--ops-kpi-min-height, 96px);
}
.ops-stat-card--interactive {
  cursor: pointer;
}
.ops-stat-card__main {
  display: inline-flex;
  align-items: baseline;
  min-width: 0;
}
.ops-stat-card__value {
  font-size: 18px;
  font-weight: 600;
  line-height: 1.25;
  font-variant-numeric: tabular-nums;
}
.ops-stat-card__value--empty {
  color: var(--el-text-color-secondary);
  font-weight: 500;
}
.ops-stat-card__unit {
  margin-left: 4px;
  color: var(--el-text-color-secondary);
  font-size: 12px;
}
.ops-stat-card__hint {
  display: block;
  margin-top: 4px;
  color: var(--el-text-color-secondary);
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
