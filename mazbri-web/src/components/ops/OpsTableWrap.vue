<template>
  <div class="ops-table-wrap">
    <slot />
  </div>
</template>

<script setup lang="ts">
defineOptions({ name: "OpsTableWrap" });
</script>

<style scoped>
.ops-table-wrap {
  width: 100%;
  min-width: 0;
  overflow-x: auto;
  overflow-y: hidden;
}
.ops-table-wrap :deep(.el-table) {
  width: 100%;
}
.ops-table-wrap :deep(.el-table .cell) {
  word-break: keep-all;
  white-space: nowrap;
}
.ops-table-wrap :deep(.el-table__header .cell) {
  white-space: nowrap;
  line-height: 1.3;
}
</style>
