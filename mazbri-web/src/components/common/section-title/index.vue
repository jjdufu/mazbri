<template>
  <div class="common-section-title" :style="titleStyles">
    {{ title }}
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";

defineOptions({ name: "CommonSectionTitle" });

const props = withDefaults(
  defineProps<{
    title: string;
    marginTop?: string;
    marginBottom?: string;
    color?: string;
  }>(),
  {
    marginTop: "0",
    marginBottom: "10px",
    color: "var(--art-text-gray-900)",
  },
);

const titleStyles = computed(() => ({
  "--common-section-title-margin-top": props.marginTop,
  "--common-section-title-margin-bottom": props.marginBottom,
  "--common-section-title-color": props.color,
}));
</script>

<style scoped lang="scss">
.common-section-title {
  margin-top: var(--common-section-title-margin-top);
  margin-bottom: var(--common-section-title-margin-bottom);
  color: var(--common-section-title-color);
  font-size: 14px;
  font-weight: 600;
}
</style>
