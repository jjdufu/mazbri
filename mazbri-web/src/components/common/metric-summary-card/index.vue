<template>
  <ElCard v-loading="loading" shadow="hover" class="metric-summary-card">
    <div class="metric-summary-card__content">
      <div
        class="metric-summary-card__icon"
        :style="{ backgroundColor: iconBackground }"
      >
        <ArtSvgIcon
          :icon="icon"
          class="text-xl"
          :style="{ color: iconColor }"
        />
      </div>
      <div class="metric-summary-card__info">
        <span class="metric-summary-card__label">{{ label }}</span>
        <slot>
          <span class="metric-summary-card__value">{{ value }}</span>
        </slot>
      </div>
    </div>
  </ElCard>
</template>

<script setup lang="ts">
defineOptions({ name: "MetricSummaryCard" });
withDefaults(
  defineProps<{
    label: string;
    icon: string;
    iconColor: string;
    iconBackground: string;
    value?: string | number;
    loading?: boolean;
  }>(),
  {
    value: "",
    loading: false,
  },
);
</script>

<style scoped lang="scss">
.metric-summary-card {
  :deep(.el-card__body) {
    padding: 16px 20px;
  }
}

.metric-summary-card__content {
  display: flex;
  align-items: center;
  gap: 16px;
}

.metric-summary-card__icon {
  width: 48px;
  height: 48px;
  border-radius: var(--el-border-radius-small);
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.metric-summary-card__info {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.metric-summary-card__label {
  font-size: 14px;
  color: var(--text-color-secondary);
}

.metric-summary-card__value,
:deep(.metric-summary-card__value) {
  font-size: 24px;
  font-weight: 600;
  color: var(--text-color-primary);
}
</style>
