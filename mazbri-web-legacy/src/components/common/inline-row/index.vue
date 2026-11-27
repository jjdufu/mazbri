<template>
  <div :class="rowClasses" :style="rowStyles">
    <slot />
  </div>
</template>

<script setup lang="ts">
defineOptions({ name: "CommonInlineRow" });

const props = withDefaults(
  defineProps<{
    gap?: string;
    minHeight?: string;
    wrap?: boolean;
    align?: "center" | "start" | "end" | "stretch";
  }>(),
  {
    gap: "10px",
    minHeight: "0",
    wrap: false,
    align: "center",
  },
);

const rowClasses = computed(() => ({
  "common-inline-row": true,
  "common-inline-row--wrap": props.wrap,
  [`common-inline-row--align-${props.align}`]: true,
}));

const rowStyles = computed(() => ({
  "--common-inline-row-gap": props.gap,
  "--common-inline-row-min-height": props.minHeight,
}));
</script>

<style scoped lang="scss">
.common-inline-row {
  display: flex;
  align-items: center;
  gap: var(--common-inline-row-gap);
  min-height: var(--common-inline-row-min-height);
}

.common-inline-row--wrap {
  flex-wrap: wrap;
}

.common-inline-row--align-start {
  align-items: flex-start;
}

.common-inline-row--align-end {
  align-items: flex-end;
}

.common-inline-row--align-stretch {
  align-items: stretch;
}
</style>
