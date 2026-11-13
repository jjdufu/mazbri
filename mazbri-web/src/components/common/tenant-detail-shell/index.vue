<template>
  <div class="tenant-detail-shell">
    <div class="tenant-detail-shell__header">
      <div v-if="$slots.identity" class="tenant-detail-shell__identity">
        <slot name="identity" />
      </div>
      <div v-if="$slots.tabs" class="tenant-detail-shell__bar">
        <div class="tenant-detail-shell__tabs">
          <slot name="tabs" />
        </div>
      </div>
    </div>

    <section class="tenant-detail-shell__content">
      <slot />
    </section>
  </div>
</template>

<script setup lang="ts">
/**
 * [不可更改] 租户详情统一页壳：
 * 1. 顶部采用紧凑双层头部：第一行常驻展示租户身份条（名称 + 编码复制 + 状态徽标），第二行纯净承载业务 Tab；
 * 2. 严禁渲染与顶部面包屑同名的“租户详情”重复大标题；
 * 3. 严禁通过 provide("opsTabHeaderActionsEl" / "resourceTabHeaderActionsEl") 将子表按钮跨层 Teleport 塞入 Tab 导航条。
 */
defineOptions({ name: "TenantDetailShell" });
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
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}

.tenant-detail-shell__identity {
  display: flex;
  flex-wrap: nowrap;
  gap: 10px;
  align-items: center;
  min-width: 0;
  min-height: 24px;
  max-height: 28px;
}

.tenant-detail-shell__bar {
  display: flex;
  align-items: center;
  min-width: 0;
  min-height: 36px;
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
