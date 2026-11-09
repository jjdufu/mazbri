<!--
  [不可更改] 页面标题统一由顶部导航栏面包屑（ArtBreadcrumb）承担。
  严禁在 OpsPage 或任何业务子页面内部渲染重复的大标题（h1/h2/.ops-page__title），
  以保证全站信息密度与导航层级唯一性。今后开发新页面必须遵守此规则。
-->
<template>
  <div class="ops-page">
    <div v-if="$slots.actions" class="ops-page__toolbar">
      <div class="ops-page__actions">
        <slot name="actions" />
      </div>
    </div>
    <slot />
  </div>
</template>

<script setup lang="ts">
defineOptions({ name: "OpsPage" });
// [不可更改] title 与 subtitle 属性仅保留签名避免调用方传参报错，严禁在模板中渲染页面级大标题或孤立说明图标。
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
  justify-content: flex-end;
  flex-shrink: 0;
}
.ops-page__actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--ops-header-actions-gap, 8px);
  align-items: center;
  flex-shrink: 0;
  margin-left: auto;
}
</style>
