<template>
  <div class="tenant-tab-shell" :class="{ 'is-loading': loading }">
    <div v-if="title || $slots.title || $slots.actions" class="tenant-tab-shell__head">
      <div class="tenant-tab-shell__heading">
        <h3 v-if="title || $slots.title" class="tenant-tab-shell__title">
          <slot name="title">{{ title }}</slot>
        </h3>
        <p v-if="subtitle || $slots.subtitle" class="tenant-tab-shell__subtitle">
          <slot name="subtitle">{{ subtitle }}</slot>
        </p>
      </div>
      <div v-if="$slots.actions" class="tenant-tab-shell__actions">
        <slot name="actions" />
      </div>
    </div>
    <div v-loading="loading" class="tenant-tab-shell__body ops-tab-page-grid">
      <slot />
    </div>
  </div>
</template>

<script setup lang="ts">
defineOptions({ name: "TenantTabShell" });

withDefaults(
  defineProps<{
    title?: string;
    subtitle?: string;
    loading?: boolean;
  }>(),
  {
    title: undefined,
    subtitle: undefined,
    loading: false,
  },
);
</script>

<style scoped lang="scss">
.tenant-tab-shell {
  display: flex;
  flex-direction: column;
  gap: var(--ops-section-gap, 16px);
  min-width: 0;
}

.tenant-tab-shell__head {
  display: flex;
  flex-wrap: wrap;
  gap: var(--ops-space-sm, 12px);
  align-items: flex-start;
  justify-content: space-between;
}

.tenant-tab-shell__heading {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.tenant-tab-shell__title {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
  line-height: 1.3;
}

.tenant-tab-shell__subtitle {
  margin: 0;
  color: var(--el-text-color-secondary);
  font-size: 12px;
  line-height: 1.4;
}

.tenant-tab-shell__actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--ops-header-actions-gap, 8px);
  align-items: center;
}

.tenant-tab-shell__body {
  min-width: 0;
}
</style>
