<template>
  <div class="ops-page art-full-height">
    <div
      v-if="title || $slots.title || $slots.actions"
      class="ops-page__toolbar"
    >
      <h2 v-if="title || $slots.title" class="ops-page__title">
        <slot name="title">{{ title }}</slot>
      </h2>
      <div v-if="$slots.actions" class="ops-page__actions">
        <slot name="actions" />
      </div>
    </div>
    <slot />
  </div>
</template>

<script setup lang="ts">
defineOptions({ name: "OpsPage" });
defineProps<{ title?: string }>();
</script>

<style scoped>
.ops-page {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 16px;
  min-width: 0;
}
.ops-page__toolbar {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  align-items: center;
  justify-content: space-between;
}
.ops-page__title {
  margin: 0;
  font-size: 18px;
  font-weight: 600;
  line-height: 1.3;
}
.ops-page__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}
</style>
