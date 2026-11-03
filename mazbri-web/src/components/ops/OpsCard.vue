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
  padding: var(--ops-space-sm, 12px) var(--ops-space-md, 16px);
  border-radius: 8px;
  background: var(--el-bg-color);
  border: 1px solid var(--el-border-color-lighter);
  min-width: 0;
  /* 图表防溢出；禁止在父级固定高度 flex 列里被压扁（否则首卡只剩 padding） */
  overflow: hidden;
  flex-shrink: 0;
}
.ops-card__head {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
  margin-bottom: var(--ops-space-xs, 8px);
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
