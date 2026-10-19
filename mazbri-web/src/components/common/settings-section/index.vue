<template>
  <section class="settings-section">
    <h3 class="settings-section__title">{{ title }}</h3>
    <div class="settings-section__content">
      <slot />
    </div>
  </section>
</template>

<script setup lang="ts">
defineOptions({ name: "SettingsSection" });
defineProps<{
  title: string;
}>();
</script>

<style scoped lang="scss">
.settings-section {
  margin-bottom: 0;
}

.settings-section__title {
  margin: 0 0 16px;
  padding-bottom: 8px;
  color: var(--el-text-color-primary);
  font-size: 14px;
  font-weight: 600;
  border-bottom: 1px solid var(--el-border-color-lighter);
}

.settings-section__content {
  padding-left: 0;
}
</style>
