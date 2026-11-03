<template>
  <ElRow :gutter="gutter" class="ops-kpi-row">
    <slot />
  </ElRow>
</template>

<script setup lang="ts">
defineOptions({ name: "OpsKpiRow" });

withDefaults(
  defineProps<{
    /** Element Plus gutter；默认对齐 --ops-kpi-gutter。 */
    gutter?: number;
  }>(),
  {
    gutter: 16,
  },
);
</script>
