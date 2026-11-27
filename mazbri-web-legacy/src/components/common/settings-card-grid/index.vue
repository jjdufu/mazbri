<template>
  <div class="settings-card-grid">
    <div class="settings-card-grid__col">
      <slot name="left" />
    </div>
    <div class="settings-card-grid__col">
      <slot name="right" />
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * [不可更改] 控制台设置页的双列卡片栅格（系统配置 / 账号安全 / 租户设置类 tab 共用）。
 * 两列等宽、顶部对齐（align-items: flex-start），严禁等高拉伸导致展开单侧折叠面板时另一侧出现大面积空白；≤1100px 时单列。
 */
defineOptions({ name: "SettingsCardGrid" });
</script>

<style scoped lang="scss">
.settings-card-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 16px;
  align-items: flex-start;
}

.settings-card-grid__col {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-width: 0;
}

@media (max-width: 1100px) {
  .settings-card-grid {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
