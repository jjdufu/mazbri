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
 * 控制台设置页的双列卡片栅格（系统配置 / 账号安全 / 租户设置类 tab 共用）。
 * 两列等宽；每列最后一张卡片拉伸填满，保证左右两列底部平齐；≤1100px 时单列。
 */
defineOptions({ name: "SettingsCardGrid" });
</script>

<style scoped lang="scss">
.settings-card-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 16px;
  align-items: stretch;
}

.settings-card-grid__col {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-width: 0;
  height: 100%;

  > :deep(:last-child) {
    flex: 1;
  }
}

@media (max-width: 1100px) {
  .settings-card-grid {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
