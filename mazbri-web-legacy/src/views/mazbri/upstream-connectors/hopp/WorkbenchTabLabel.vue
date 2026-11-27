<template>
  <span class="wb-tab-label" :class="{ 'is-active': active }">
    {{ label }}
    <span v-if="count" class="wb-tab-count">{{ count }}</span>
    <span v-else-if="dot" class="wb-tab-dot" />
  </span>
</template>

<script setup lang="ts">
withDefaults(
  defineProps<{
    label: string;
    count?: number;
    dot?: boolean;
    active?: boolean;
  }>(),
  { count: 0, dot: false, active: false },
);
</script>

<style scoped lang="scss">
.wb-tab-label {
  display: inline-flex;
  align-items: center;
  white-space: nowrap;
}
.wb-tab-count {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 1rem;
  height: 16px;
  margin-left: 8px;
  padding: 0 4px;
  font-size: 8px;
  font-weight: 600;
  line-height: 1;
  color: var(--wb-text-faint, var(--el-text-color-placeholder));
  border: 1px solid var(--wb-divider-subtle, var(--el-border-color-extra-light));
  border-radius: var(--wb-radius, var(--art-form-control-radius));
}
.wb-tab-label.is-active .wb-tab-count {
  color: var(--wb-text-muted, var(--el-text-color-secondary));
  border-color: var(--wb-divider, var(--el-border-color));
}
.wb-tab-dot {
  display: inline-block;
  width: 6px;
  height: 6px;
  margin-left: 8px;
  border-radius: 50%;
  background: var(--wb-accent, var(--el-color-primary));
  vertical-align: middle;
}
</style>
