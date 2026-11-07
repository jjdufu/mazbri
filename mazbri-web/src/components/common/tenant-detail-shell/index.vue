<template>
  <div class="tenant-detail-shell">
    <header class="tenant-detail-shell__header">
      <div class="tenant-detail-shell__header-main">
        <slot name="header">
          <h1 v-if="title" class="tenant-detail-shell__title">{{ title }}</h1>
        </slot>
      </div>
      <div v-if="$slots.extra" class="tenant-detail-shell__extra">
        <slot name="extra" />
      </div>
    </header>

    <div v-if="$slots.tabs" class="tenant-detail-shell__tabs">
      <slot name="tabs" />
    </div>

    <section class="tenant-detail-shell__content">
      <slot />
    </section>
  </div>
</template>

<script setup lang="ts">
defineOptions({ name: "TenantDetailShell" });

defineProps<{ title?: string }>();
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

.tenant-detail-shell__header {
  display: flex;
  flex-wrap: wrap;
  gap: var(--ops-space-sm, 12px);
  align-items: center;
  justify-content: space-between;
}

.tenant-detail-shell__header-main {
  min-width: 0;
  flex: 1;
}

.tenant-detail-shell__title {
  margin: 0;
  font-size: 18px;
  font-weight: 600;
  line-height: 1.3;
}

.tenant-detail-shell__extra {
  display: flex;
  flex-wrap: wrap;
  gap: var(--ops-header-actions-gap, 8px);
  align-items: center;
}

.tenant-detail-shell__tabs {
  flex-shrink: 0;

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
