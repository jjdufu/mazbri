<template>
  <div class="ops-page">
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
/* 不要挂 art-full-height：固定 height + 纵向 flex 会在内容超高时把
   带 overflow:hidden 的 OpsCard（首卡尤甚）压扁，表现为「第一个容器被截断」。
   页面滚动交给 #app-main；此处只做最小可视高度，不限制增长。 */
.ops-page {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 16px;
  min-width: 0;
  min-height: var(--art-full-height, calc(100vh - 48px));
  box-sizing: border-box;
}
.ops-page__toolbar {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  align-items: center;
  justify-content: space-between;
  flex-shrink: 0;
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
