<template>
  <component :is="tag" :class="blockClasses">
    <slot />
  </component>
</template>

<script setup lang="ts">
import { computed } from "vue";

defineOptions({ name: "CommonSettingsBlock" });

const props = withDefaults(
  defineProps<{
    tag?: string;
    variant?: "system" | "display" | "theme" | "layout" | "style";
  }>(),
  {
    tag: "div",
    variant: "system",
  },
);

const blockClasses = computed(() => [
  "settings-block",
  `settings-block--${props.variant}`,
]);
</script>
