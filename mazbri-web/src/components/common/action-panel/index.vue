<template>
  <div :class="panelClasses" :style="panelStyles">
    <div v-if="$slots.title || title" class="common-action-panel-title">
      <slot name="title">
        {{ title }}
      </slot>
    </div>

    <div v-if="$slots.hint || hint" class="common-action-panel-hint">
      <slot name="hint">
        {{ hint }}
      </slot>
    </div>

    <slot />

    <div v-if="$slots.actions" class="common-action-panel-actions">
      <slot name="actions" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";

defineOptions({ name: "CommonActionPanel" });

const props = withDefaults(
  defineProps<{
    title?: string;
    hint?: string;
    surface?: boolean;
    fill?: boolean;
    gap?: string;
    align?: "start" | "center" | "end" | "stretch";
    surfaceMinWidth?: string;
  }>(),
  {
    title: "",
    hint: "",
    surface: false,
    fill: false,
    gap: "12px",
    align: "stretch",
    surfaceMinWidth: "220px",
  },
);

const panelClasses = computed(() => ({
  "common-action-panel": true,
  "common-action-panel--surface": props.surface,
  "common-action-panel--fill": props.fill,
  [`common-action-panel--align-${props.align}`]: true,
}));

const panelStyles = computed(() => ({
  "--common-action-panel-gap": props.gap,
  "--common-action-panel-surface-min-width": props.surfaceMinWidth,
}));
</script>

<style scoped lang="scss">
.common-action-panel {
  display: flex;
  flex-direction: column;
  gap: var(--common-action-panel-gap);
  min-width: 0;
}

.common-action-panel--fill {
  width: 100%;
}

.common-action-panel--align-start {
  align-items: flex-start;
}

.common-action-panel--align-center {
  align-items: center;
}

.common-action-panel--align-end {
  align-items: flex-end;
}

.common-action-panel--align-stretch {
  align-items: stretch;
}

.common-action-panel--surface {
  padding: 16px 18px;
  border-radius: var(--el-border-radius-base);
  background: var(--el-fill-color-light);
  min-width: var(--common-action-panel-surface-min-width);
}

.common-action-panel-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-color-primary);
}

.common-action-panel-hint {
  font-size: 13px;
  line-height: 1.5;
  color: var(--text-color-secondary);
}

.common-action-panel-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
}

@media (max-width: 768px) {
  .common-action-panel--surface {
    width: 100%;
    min-width: 0;
  }
}
</style>
