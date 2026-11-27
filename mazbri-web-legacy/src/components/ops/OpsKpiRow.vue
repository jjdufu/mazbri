<template>
  <div class="ops-kpi-row" :data-cols="columns">
    <slot />
  </div>
</template>

<script setup lang="ts">
defineOptions({ name: "OpsKpiRow" });

withDefaults(
  defineProps<{
    /** 可选固定列数（6 卡行在桌面 6 列、中屏 3×2 防 5+1 孤卡；未传则 auto-fit 等分）。 */
    columns?: 3 | 4 | 6;
  }>(),
  {
    columns: undefined,
  },
);
</script>
