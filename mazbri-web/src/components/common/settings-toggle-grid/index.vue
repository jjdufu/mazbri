<template>
  <div class="settings-toggle-grid">
    <slot />
  </div>
</template>

<script setup lang="ts">
/**
 * 设置卡片内的开关小网格：两列排布 CommonToggleTile（如系统配置「显示设置」、
 * 账号安全「密码复杂度」），去掉 tile 自带的边框底色，与卡片行风格一致。
 */
defineOptions({ name: "SettingsToggleGrid" });
</script>

<style scoped lang="scss">
.settings-toggle-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px 24px;

  :deep(.common-toggle-tile) {
    min-height: 0;
    padding: 6px 0;
    background: transparent;
    border: none;
    border-radius: 0;
  }

  :deep(.common-toggle-tile__label) {
    font-size: 14px;
    font-weight: 500;
  }
}

@media (max-width: 640px) {
  .settings-toggle-grid {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
