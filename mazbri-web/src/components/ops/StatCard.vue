<template>
  <MetricSummaryCard
    class="ops-stat-card"
    :label="label"
    :icon="icon"
    :icon-color="iconColor"
    :icon-background="iconBackground"
    :loading="loading"
    @click="$emit('click', $event)"
  >
    <span class="ops-stat-card__value">{{ displayValue }}</span>
    <span v-if="unit" class="ops-stat-card__unit">{{ unit }}</span>
    <span v-if="hint" class="ops-stat-card__hint">{{ hint }}</span>
  </MetricSummaryCard>
</template>

<script setup lang="ts">
import { computed } from "vue";
import MetricSummaryCard from "@/components/common/metric-summary-card/index.vue";

defineEmits<{ click: [MouseEvent] }>();

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
    /** Hide numeric amounts (viewer slim mode). */
    hideAmount?: boolean;
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
    hideAmount: false,
    digits: 2,
  },
);

const displayValue = computed(() => {
  if (props.hideAmount) return "—";
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
</script>

<style scoped>
.ops-stat-card__value {
  font-size: 22px;
  font-weight: 600;
  line-height: 1.2;
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
}
</style>
