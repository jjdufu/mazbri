<template>
  <div class="ops-page">
    <div
      v-if="title || $slots.title || subtitle || $slots.subtitle || $slots.actions"
      class="ops-page__toolbar"
    >
      <div class="ops-page__heading">
        <div class="ops-page__title-row">
          <h2 v-if="title || $slots.title" class="ops-page__title">
            <slot name="title">{{ title }}</slot>
          </h2>
          <ElPopover
            v-if="subtitle || $slots.subtitle"
            placement="bottom-start"
            :width="320"
            trigger="click"
          >
            <div class="ops-page__info-popover">
              <slot name="subtitle">{{ subtitle }}</slot>
            </div>
            <template #reference>
              <button
                type="button"
                class="ops-page__info-trigger"
                aria-label="页面说明"
              >
                <ArtSvgIcon icon="ri:information-line" />
              </button>
            </template>
          </ElPopover>
        </div>
      </div>
      <div v-if="$slots.actions" class="ops-page__actions">
        <slot name="actions" />
      </div>
    </div>
    <slot />
  </div>
</template>

<script setup lang="ts">
defineOptions({ name: "OpsPage" });
defineProps<{ title?: string; subtitle?: string }>();
</script>

<style scoped>
/* 不要挂 art-full-height：固定 height + 纵向 flex 会在内容超高时把
   带 overflow:hidden 的 OpsCard（首卡尤甚）压扁，表现为「第一个容器被截断」。
   页面滚动交给 #app-main；此处只做最小可视高度，不限制增长。 */
.ops-page {
  display: flex;
  flex-direction: column;
  gap: var(--ops-page-gap, 16px);
  padding: var(--ops-page-padding, 16px);
  min-width: 0;
  min-height: var(--art-full-height, calc(100vh - 48px));
  box-sizing: border-box;
}
.ops-page__toolbar {
  display: flex;
  flex-wrap: wrap;
  gap: var(--ops-space-sm, 12px);
  align-items: center;
  justify-content: space-between;
  flex-shrink: 0;
}
.ops-page__heading {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
  flex: 1;
}
.ops-page__title-row {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  min-width: 0;
}
.ops-page__title {
  margin: 0;
  font-size: 18px;
  font-weight: 600;
  line-height: 1.3;
}
.ops-page__info-trigger {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  padding: 0;
  margin-top: -4px;
  border: none;
  border-radius: 50%;
  background: transparent;
  color: var(--el-text-color-secondary);
  font-size: 14px;
  cursor: pointer;
  transition: color 0.15s ease, background-color 0.15s ease;
}
.ops-page__info-trigger:hover {
  color: var(--el-color-primary);
  background: var(--el-fill-color-light);
}
.ops-page__info-popover {
  color: var(--el-text-color-regular);
  font-size: 12px;
  line-height: 1.5;
}
.ops-page__actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--ops-header-actions-gap, 8px);
  align-items: center;
  flex-shrink: 0;
}
</style>
