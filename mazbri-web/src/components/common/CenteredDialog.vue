<template>
  <ElDialog
    v-model="visible"
    :title="props.title"
    :width="props.width"
    :show-close="props.showClose"
    :close-on-click-modal="props.closeOnClickModal"
    :close-on-press-escape="props.closeOnPressEscape"
    :destroy-on-close="props.destroyOnClose"
    :lock-scroll="props.lockScroll"
    :append-to-body="props.appendToBody"
    :modal-class="props.modalClass"
    :class="[
      'centered-dialog',
      props.dialogClass,
      { 'centered-dialog--hide-header': props.hideHeader },
    ]"
    align-center
    @open="emit('open')"
    @opened="emit('opened')"
    @close="emit('close')"
    @closed="emit('closed')"
  >
    <slot />

    <template v-if="$slots.footer" #footer>
      <slot name="footer" />
    </template>
  </ElDialog>
</template>

<script setup lang="ts">
interface Props {
  title?: string;
  width?: string | number;
  showClose?: boolean;
  closeOnClickModal?: boolean;
  closeOnPressEscape?: boolean;
  destroyOnClose?: boolean;
  lockScroll?: boolean;
  appendToBody?: boolean;
  modalClass?: string;
  dialogClass?: string;
  hideHeader?: boolean;
}
const props = withDefaults(defineProps<Props>(), {
  title: "",
  width: "640px",
  showClose: true,
  closeOnClickModal: true,
  closeOnPressEscape: true,
  destroyOnClose: false,
  lockScroll: true,
  appendToBody: true,
  modalClass: "",
  dialogClass: "",
  hideHeader: false,
});
const emit = defineEmits<{
  open: [];
  opened: [];
  close: [];
  closed: [];
}>();
const visible = defineModel<boolean>({ required: true });
</script>

<style scoped lang="scss">
/* [不可更改] 居中弹窗统一绑定 Element Plus 语义变量，严禁硬编码亮/暗色 Hex 或添加标题左竖条与关闭按钮旋转 */
.centered-dialog {
  :deep(.el-dialog) {
    max-width: calc(100vw - 32px);
    border-radius: var(--art-dialog-radius);
    overflow: hidden;
    box-shadow: var(--el-box-shadow-dark);
    border: 1px solid var(--el-border-color-lighter);
    background: var(--el-bg-color-overlay);
    transition:
      border-color 0.2s ease,
      background-color 0.2s ease;
  }

  :deep(.el-dialog__header) {
    margin-right: 0;
    padding: 20px 24px 16px;
    border-bottom: 1px solid var(--el-border-color-lighter);
    display: flex;
    align-items: center;
    justify-content: space-between;
    background: var(--el-bg-color-overlay);

    .el-dialog__title {
      font-size: 16px;
      font-weight: 600;
      color: var(--el-text-color-primary);
      letter-spacing: 0.01em;
      display: inline-flex;
      align-items: center;
      gap: 8px;
    }

    .el-dialog__headerbtn {
      top: 0;
      position: relative;
      width: 32px;
      height: 32px;
      border-radius: var(--custom-radius);
      display: flex;
      align-items: center;
      justify-content: center;
      transition: background-color 0.2s ease;
      margin: 0;

      &:hover {
        background: var(--el-fill-color-light);

        .el-dialog__close {
          color: var(--el-text-color-primary);
        }
      }

      .el-dialog__close {
        font-size: 16px;
        color: var(--el-text-color-secondary);
        transition: color 0.2s ease;
      }
    }
  }

  :deep(.el-dialog__body) {
    box-sizing: border-box;
    padding: 24px 28px;
  }

  :deep(.el-dialog__footer) {
    padding: 16px 28px 20px;
    border-top: 1px solid var(--el-border-color-lighter);
    background: var(--el-bg-color-overlay);
  }
}

.centered-dialog--hide-header {
  :deep(.el-dialog__header) {
    display: none;
  }
}
</style>
