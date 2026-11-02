<template>
  <div class="ops-card">
    <div
      v-if="title || $slots.title || $slots.extra"
      class="ops-card__head"
    >
      <h3 v-if="title || $slots.title" class="ops-card__title">
        <slot name="title">{{ title }}</slot>
      </h3>
      <div v-if="$slots.extra" class="ops-card__extra">
        <slot name="extra" />
      </div>
    </div>
    <div class="ops-card__body">
      <slot />
    </div>
  </div>
</template>

<script setup lang="ts">
defineOptions({ name: "OpsCard" });
defineProps<{ title?: string }>();
</script>

<style scoped>
.ops-card {
  margin-bottom: 12px;
  padding: 12px 16px;
  border-radius: 8px;
  background: var(--el-bg-color);
  border: 1px solid var(--el-border-color-lighter);
  min-width: 0;
  overflow: hidden;
}
.ops-card__head {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
  margin-bottom: 8px;
}
.ops-card__title {
  margin: 0;
  margin-right: auto;
  font-size: 14px;
  font-weight: 600;
  line-height: 1.3;
}
.ops-card__extra {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}
.ops-card__body {
  min-width: 0;
}
</style>
