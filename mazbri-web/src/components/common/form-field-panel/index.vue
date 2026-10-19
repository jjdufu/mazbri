<template>
  <ElFormItem
    :label="label"
    :prop="prop"
    class="common-form-field-panel"
    :class="{ 'common-form-field-panel--compact': compact }"
  >
    <slot />
  </ElFormItem>
</template>

<script setup lang="ts">
defineOptions({ name: "CommonFormFieldPanel" });
withDefaults(
  defineProps<{
    label?: string;
    prop?: string;
    compact?: boolean;
  }>(),
  {
    label: "",
    prop: "",
    compact: false,
  },
);
</script>

<style scoped lang="scss">
.common-form-field-panel {
  margin-bottom: 0;
  padding: 0;
  background: transparent;

  :deep(.el-form-item__label) {
    padding-bottom: 6px;
    font-size: 13px;
    font-weight: 600;
    color: var(--el-text-color-primary);
  }

  :deep(.el-form-item__content) {
    display: block;
    line-height: 1.6;
  }

  :deep(.el-input-number) {
    width: 100%;
  }
}

.common-form-field-panel--compact {
  min-width: 200px;
  width: min(100%, 200px);
}
</style>
