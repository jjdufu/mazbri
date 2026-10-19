<template>
  <div class="common-stat-strip">
    <div
      v-for="(item, index) in items"
      :key="`${item.label}-${index}`"
      class="common-stat-strip__item"
    >
      <span class="common-stat-strip__label">{{ item.label }}</span>
      <span class="common-stat-strip__value">{{ item.value }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
defineOptions({ name: "CommonStatStrip" });

withDefaults(
  defineProps<{
    items: Array<{
      label: string;
      value: string;
    }>;
  }>(),
  {
    items: () => [],
  },
);
</script>

<style scoped lang="scss">
.common-stat-strip {
  display: flex;
  flex-wrap: wrap;
  gap: 12px 20px;
  padding: 2px 0 14px;
}

.common-stat-strip__item {
  display: inline-flex;
  align-items: baseline;
  gap: 8px;
  position: relative;
}

.common-stat-strip__item:not(:last-child)::after {
  content: "";
  position: absolute;
  right: -10px;
  top: 50%;
  width: 1px;
  height: 16px;
  background: var(--el-border-color-lighter);
  transform: translateY(-50%);
}

.common-stat-strip__label {
  font-size: 12px;
  color: var(--el-text-color-secondary);
}

.common-stat-strip__value {
  font-size: 18px;
  font-weight: 600;
  color: var(--el-text-color-primary);
}

@media (max-width: 768px) {
  .common-stat-strip {
    flex-direction: column;
    gap: 10px;
  }

  .common-stat-strip__item:not(:last-child)::after {
    display: none;
  }
}
</style>
