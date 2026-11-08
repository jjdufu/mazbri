<template>
  <div class="tenant-detail-shell">
    <div v-if="$slots.tabs || $slots.extra" class="tenant-detail-shell__bar">
      <div v-if="$slots.tabs" class="tenant-detail-shell__tabs">
        <slot name="tabs" />
      </div>
      <div ref="headerActionsEl" class="tenant-detail-shell__extra">
        <slot name="extra" />
      </div>
    </div>

    <section class="tenant-detail-shell__content">
      <slot />
    </section>
  </div>
</template>

<script setup lang="ts">
import { provide, ref } from "vue";

defineOptions({ name: "TenantDetailShell" });

const headerActionsEl = ref<HTMLElement | null>(null);
provide("opsTabHeaderActionsEl", headerActionsEl);
</script>

<style scoped lang="scss">
.tenant-detail-shell {
  display: flex;
  flex-direction: column;
  gap: var(--ops-section-gap, 16px);
  padding: var(--ops-page-padding, 16px);
  min-width: 0;
  box-sizing: border-box;
}

.tenant-detail-shell__bar {
  display: flex;
  flex-wrap: wrap;
  gap: var(--ops-space-sm, 12px);
  align-items: center;
  justify-content: space-between;
  min-height: 36px;
}

.tenant-detail-shell__extra {
  display: flex;
  flex-wrap: wrap;
  gap: var(--ops-header-actions-gap, 8px);
  align-items: center;
  margin-left: auto;
}

.tenant-detail-shell__tabs {
  flex: 1 1 auto;
  min-width: 0;

  :deep(.el-tabs__header) {
    margin: 0 !important;
  }

  :deep(.el-tabs__content) {
    display: none !important;
  }
}

.tenant-detail-shell__content {
  display: flex;
  flex-direction: column;
  gap: var(--ops-section-gap, 16px);
  min-width: 0;
}
</style>
