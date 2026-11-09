<template>
  <Teleport v-if="headerActionsEl" :to="headerActionsEl">
    <slot />
  </Teleport>
  <div v-else class="ops-panel-toolbar">
    <slot />
  </div>
</template>

<script setup lang="ts">
import { inject, type Ref } from "vue";

/**
 * Tab 面板内局部工具条（筛选 / 刷新 / 新建等）。
 * 若位于 OpsTabPage 内部，自动挂载到与 Tab 同行的右侧操作区，消除多余工具栏行。
 */
defineOptions({ name: "OpsPanelToolbar" });

const headerActionsEl = inject<Ref<HTMLElement | null> | null>(
  "opsTabHeaderActionsEl",
  null,
);
</script>

<style scoped>
.ops-panel-toolbar {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
  justify-content: flex-end;
}
</style>
