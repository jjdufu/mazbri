<template>
  <div class="ops-card">
    <div
      v-if="title || $slots.title || info || $slots.info || $slots.extra"
      class="ops-card__head"
    >
      <div v-if="title || $slots.title || info || $slots.info" class="ops-card__title-wrap">
        <h3 v-if="title || $slots.title" class="ops-card__title">
          <slot name="title">{{ title }}</slot>
        </h3>
        <ElPopover
          v-if="info || $slots.info"
          placement="bottom-start"
          :width="300"
          trigger="click"
        >
          <div class="ops-card__info-popover">
            <slot name="info">{{ info }}</slot>
          </div>
          <template #reference>
            <button
              type="button"
              class="ops-card__info-trigger"
              aria-label="说明"
            >
              <ArtSvgIcon icon="ri:information-line" />
            </button>
          </template>
        </ElPopover>
      </div>
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
defineProps<{ title?: string; info?: string }>();
</script>

<style scoped>
.ops-card {
  padding: var(--ops-space-sm, 12px) var(--ops-space-md, 16px);
  border-radius: var(--art-card-radius);
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
  padding-bottom: 10px;
  margin-bottom: 12px;
  border-bottom: 1px solid var(--el-border-color-extra-light);
}
.ops-card__title-wrap {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  margin-right: auto;
  min-width: 0;
}
.ops-card__title {
  margin: 0;
  font-size: 14px;
  font-weight: 600;
  line-height: 1.3;
  color: var(--el-text-color-primary);
}
.ops-card__info-trigger {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  padding: 0;
  margin-top: -2px;
  border: none;
  border-radius: 50%;
  background: transparent;
  color: var(--el-text-color-secondary);
  font-size: 13px;
  cursor: pointer;
  transition: color 0.15s ease, background-color 0.15s ease;
}
.ops-card__info-trigger:hover {
  color: var(--el-color-primary);
  background: var(--el-fill-color-light);
}
.ops-card__info-popover {
  color: var(--el-text-color-regular);
  font-size: 12px;
  line-height: 1.5;
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
