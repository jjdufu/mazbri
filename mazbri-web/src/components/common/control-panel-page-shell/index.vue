<!--
  [不可更改] 页面主标题统一由顶部导航栏面包屑（ArtBreadcrumb）展示。
  严禁在 ControlPanelPageShell 或业务页面内部渲染重复的大标题（h1/h2/.control-panel-page-shell__title），
  以保证页面展示信息密度。今后开发新页面必须遵守此规则。
-->
<template>
  <div class="control-panel-page-shell">
    <section
      v-if="subtitle || slots.subtitle || slots.titleExtra || slots.actions"
      class="control-panel-page-shell__header"
    >
      <div
        v-if="subtitle || slots.subtitle || slots.titleExtra"
        class="control-panel-page-shell__copy"
      >
        <div class="control-panel-page-shell__title-wrap">
          <span
            v-if="subtitle || slots.subtitle"
            class="control-panel-page-shell__subtitle"
          >
            <slot name="subtitle">{{ subtitle }}</slot>
          </span>
          <slot name="titleExtra" />
        </div>
      </div>

      <div v-if="slots.actions" class="control-panel-page-shell__actions">
        <slot name="actions" />
      </div>
    </section>

    <section
      v-if="slots.toolbar"
      class="control-panel-page-shell__toolbar surface-toolbar-card"
    >
      <slot name="toolbar" />
    </section>

    <section class="control-panel-page-shell__body">
      <slot />
    </section>
  </div>
</template>

<script setup lang="ts">
defineOptions({ name: "ControlPanelPageShell" });
interface Props {
  // [不可更改] title 与 icon 仅保留签名兼容调用方，严禁在模板中渲染页面级大标题。
  title?: string;
  subtitle?: string;
  icon?: string;
}
withDefaults(defineProps<Props>(), {
  title: "",
  subtitle: "",
  icon: "",
});
const slots = useSlots();
</script>

<style scoped lang="scss">
.control-panel-page-shell {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 12px 16px;
  width: 100%;
  min-width: 0;

  &__header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    min-width: 0;
    padding: 2px 2px 0;
  }

  &__copy {
    min-width: 0;
  }

  &__title-wrap {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 6px 12px;
    min-width: 0;
  }

  &__subtitle {
    color: var(--art-gray-600);
    font-size: 13px;
    font-weight: 400;
    line-height: 1.4;
  }

  &__actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
    margin-left: auto;
  }

  &__toolbar {
    position: relative;
    overflow: hidden;
  }

  &__body {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    gap: 12px;
    width: 100%;
    min-width: 0;
  }

  :deep(.control-panel-toolbar-form.el-form--inline) {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 12px 16px;
  }

  :deep(.control-panel-toolbar-form .el-form-item) {
    margin-right: 0;
    margin-bottom: 0;
  }

  :deep(.control-panel-toolbar-form .el-form-item__label) {
    color: var(--art-gray-700);
    font-weight: 600;
  }

  @media (max-width: 900px) {
    &__header {
      flex-direction: column;
      align-items: flex-start;
    }

    &__actions {
      width: 100%;
      justify-content: flex-start;
    }
  }

  @media (max-width: 768px) {
    padding: 12px;
    gap: 12px;
  }
}
</style>
