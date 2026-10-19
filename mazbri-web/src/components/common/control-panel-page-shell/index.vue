<template>
  <div class="control-panel-page-shell">
    <section
      v-if="title || slots.actions"
      class="control-panel-page-shell__header"
    >
      <div class="control-panel-page-shell__copy">
        <div v-if="title" class="control-panel-page-shell__title-row">
          <ArtSvgIcon
            :icon="icon"
            class="control-panel-page-shell__title-icon"
          />
          <div class="control-panel-page-shell__title-wrap">
            <h1 class="control-panel-page-shell__title">{{ title }}</h1>
            <span
              v-if="subtitle || slots.subtitle"
              class="control-panel-page-shell__subtitle"
            >
              <slot name="subtitle">{{ subtitle }}</slot>
            </span>
            <slot name="titleExtra" />
          </div>
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
  title?: string;

  subtitle?: string;
  icon: string;
}
withDefaults(defineProps<Props>(), {
  title: "",
  subtitle: "",
});
const slots = useSlots();
</script>

<style scoped lang="scss">
.control-panel-page-shell {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 16px;
  width: 100%;
  min-width: 0;

  &__header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    min-width: 0;
    padding: 4px 2px 0;
  }

  &__title-row {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  &__title-icon {
    color: var(--theme-color);
    font-size: 18px;
    flex-shrink: 0;
    transform: translateY(-1px);
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

  &__title {
    margin: 0;
    color: var(--art-gray-900);
    font-size: 20px;
    font-weight: 600;
    line-height: 1.35;
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
  }

  &__toolbar {
    position: relative;
    overflow: hidden;
  }

  &__body {
    display: flex;
    flex-direction: column;
    gap: 16px;
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

    &__title {
      font-size: 18px;
    }
  }
}
</style>
