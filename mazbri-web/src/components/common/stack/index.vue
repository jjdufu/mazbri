<template>
  <div :class="stackClasses" :style="stackStyles">
    <slot />
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";

defineOptions({ name: "CommonStack" });

const props = withDefaults(
  defineProps<{
    direction?: "horizontal" | "vertical";
    gap?: string;
    fill?: boolean;
    align?: "start" | "center" | "end" | "stretch";
  }>(),
  {
    direction: "vertical",
    gap: "12px",
    fill: false,
    align: "stretch",
  },
);

const stackClasses = computed(() => ({
  "common-stack": true,
  "common-stack--fill": props.fill,
  [`common-stack--${props.direction}`]: true,
  [`common-stack--align-${props.align}`]: true,
}));

const stackStyles = computed(() => ({
  "--common-stack-gap": props.gap,
}));
</script>

<style scoped lang="scss">
.common-stack {
  display: flex;
  gap: var(--common-stack-gap);
  min-width: 0;
}

.common-stack--horizontal {
  flex-direction: row;
  align-items: center;
}

.common-stack--vertical {
  flex-direction: column;
}

.common-stack--fill {
  width: 100%;
}

.common-stack--align-start {
  align-items: flex-start;
}

.common-stack--align-center {
  align-items: center;
}

.common-stack--align-end {
  align-items: flex-end;
}

.common-stack--align-stretch {
  align-items: stretch;
}
</style>
