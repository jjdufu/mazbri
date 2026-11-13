<template>
  <ElCard v-loading="loading" shadow="hover" class="metric-summary-card">
    <div class="metric-summary-card__body">
      <div class="metric-summary-card__content">
        <div
          v-if="icon"
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
          <span class="metric-summary-card__label" :title="label">{{
            label
          }}</span>
          <slot>
            <span class="metric-summary-card__value">{{ value }}</span>
          </slot>
        </div>
      </div>
      <div v-if="$slots.footer" class="metric-summary-card__footer">
        <slot name="footer" />
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
  height: 100%;

  :deep(.el-card__body) {
    padding: 14px 16px;
    height: 100%;
    box-sizing: border-box;
  }
}

.metric-summary-card__body {
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 6px;
  min-width: 0;
  height: 100%;
}

.metric-summary-card__content {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
}

.metric-summary-card__footer {
  min-width: 0;
}

.metric-summary-card__icon {
  width: 42px;
  height: 42px;
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
  min-width: 0;
  flex: 1;
}

.metric-summary-card__label {
  font-size: 13px;
  color: var(--text-color-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}

.metric-summary-card__value,
:deep(.metric-summary-card__value) {
  font-size: 18px;
  font-weight: 600;
  color: var(--text-color-primary);
  line-height: 1.25;
}
</style>
