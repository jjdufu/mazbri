<template>
  <component :is="block ? 'div' : 'span'" :class="hintClasses">
    <slot />
  </component>
</template>

<script setup lang="ts">
import { computed } from "vue";

defineOptions({ name: "CommonInlineHint" });

const props = withDefaults(
  defineProps<{
    block?: boolean;
    size?: "xs" | "sm";
    tone?: "muted" | "primary" | "warning" | "danger";
  }>(),
  {
    block: false,
    size: "xs",
    tone: "muted",
  },
);

const hintClasses = computed(() => ({
  "common-inline-hint": true,
  "common-inline-hint--block": props.block,
  [`common-inline-hint--${props.size}`]: true,
  [`common-inline-hint--${props.tone}`]: true,
}));
</script>

<style scoped lang="scss">
.common-inline-hint {
  line-height: 1.5;
}

.common-inline-hint--block {
  display: block;
}

.common-inline-hint--xs {
  color: var(--el-text-color-secondary);
  font-size: 12px;
  line-height: 1.5;
}

.common-inline-hint--sm {
  color: var(--el-text-color-secondary);
  font-size: 14px;
  line-height: 1.4;
}

.common-inline-hint--muted {
  color: var(--el-text-color-secondary);
}

.common-inline-hint--primary {
  color: var(--el-color-primary);
}

.common-inline-hint--warning {
  color: var(--el-color-warning);
}

.common-inline-hint--danger {
  color: var(--el-color-danger);
}
</style>
