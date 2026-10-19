<template>
  <div :class="gridClasses" :style="gridStyles">
    <slot />
  </div>
</template>

<script setup lang="ts">
defineOptions({ name: "CommonFormGrid" });

const props = withDefaults(
  defineProps<{
    columns?: number;
    compact?: boolean;
    single?: boolean;
    minWidth?: number;
    gap?: string;
    maxWidthMode?: "1fr" | "max-content";
  }>(),
  {
    columns: 2,
    compact: false,
    single: false,
    minWidth: 180,
    gap: "14px",
    maxWidthMode: "1fr",
  },
);

const gridClasses = computed(() => ({
  "common-form-grid": true,
  "common-form-grid--compact": props.compact,
  "common-form-grid--single": props.single,
}));

const gridStyles = computed(() => ({
  "--common-form-grid-columns": String(props.columns),
  "--common-form-grid-min-width": `${props.minWidth}px`,
  "--common-form-grid-gap": props.gap,
  "--common-form-grid-max-width": props.maxWidthMode,
}));
</script>

<style scoped lang="scss">
.common-form-grid {
  display: grid;
  gap: var(--common-form-grid-gap);
  grid-template-columns: repeat(
    var(--common-form-grid-columns),
    minmax(0, 1fr)
  );
}

.common-form-grid--compact {
  grid-template-columns: repeat(
    auto-fit,
    minmax(var(--common-form-grid-min-width), var(--common-form-grid-max-width))
  );
}

.common-form-grid--single {
  grid-template-columns: 1fr;
}

@media (max-width: 768px) {
  .common-form-grid {
    grid-template-columns: 1fr !important;
  }
}
</style>
