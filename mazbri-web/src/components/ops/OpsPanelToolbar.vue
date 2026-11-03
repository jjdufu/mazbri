<template>
  <div class="ops-panel-toolbar">
    <slot />
  </div>
</template>

<script setup lang="ts">
/**
 * Tab 面板内局部工具条（筛选 / 刷新 / 新建等）。
 * 样式对齐 OpsPage 的 actions 行，避免每个合并面板各自抄一套 flex。
 */
defineOptions({ name: "OpsPanelToolbar" });
</script>

<style scoped>
.ops-panel-toolbar {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
  margin-bottom: 12px;
}
</style>
