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
    <span class="ops-stat-card__value">{{ displayValue }}</span>
    <span v-if="unit" class="ops-stat-card__unit">{{ unit }}</span>
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
  },
);

const displayValue = computed(() => {
  if (props.value === null || props.value === undefined || props.value === "") {
    return "—";
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
.ops-stat-card__value {
  font-size: 18px;
  font-weight: 600;
  line-height: 1.25;
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
