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
.centered-dialog {
  :deep(.el-dialog) {
    max-width: calc(100vw - 32px);
    border-radius: var(--art-dialog-radius);
    overflow: hidden;
    box-shadow:
      0 24px 64px -16px rgba(15, 23, 42, 0.12),
      0 0 1px rgba(15, 23, 42, 0.1);
    border: 1px solid rgba(241, 245, 249, 0.8);
    background: rgba(255, 255, 255, 0.98);
    backdrop-filter: blur(20px);
    transition: all 0.3s cubic-bezier(0.25, 0.8, 0.25, 1);

    .dark & {
      background: rgba(23, 23, 26, 0.95);
      border-color: rgba(255, 255, 255, 0.06);
      box-shadow:
        0 24px 64px -16px rgba(0, 0, 0, 0.6),
        0 0 1px rgba(255, 255, 255, 0.15);
    }
  }

  :deep(.el-dialog__header) {
    margin-right: 0;
    padding: 20px 24px 16px;
    border-bottom: 1px solid #f1f5f9;
    display: flex;
    align-items: center;
    justify-content: space-between;
    background: #fafbfd;

    .dark & {
      background: #18181c;
      border-color: #232326;
    }

    .el-dialog__title {
      font-size: 16px;
      font-weight: 600;
      color: #1e293b;
      letter-spacing: 0.01em;
      display: inline-flex;
      align-items: center;
      gap: 8px;

      &::before {
        content: "";
        display: inline-block;
        width: 3px;
        height: 14px;
        background-color: var(--el-color-primary);
        border-radius: 4px;
      }

      .dark & {
        color: #f1f5f9;
      }
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
      transition: all 0.2s ease;
      margin: 0;

      &:hover {
        background: #f1f5f9;

        .dark & {
          background: #2b2b30;
        }

        .el-dialog__close {
          color: #475569;
          transform: rotate(90deg);

          .dark & {
            color: #f1f5f9;
          }
        }
      }

      .el-dialog__close {
        font-size: 16px;
        color: #94a3b8;
        transition: all 0.3s ease;
      }
    }
  }

  :deep(.el-dialog__body) {
    box-sizing: border-box;
    padding: 24px 28px;
  }

  :deep(.el-dialog__footer) {
    padding: 16px 28px 20px;
    border-top: 1px solid #f1f5f9;
    background: #fafbfd;

    .dark & {
      background: #18181c;
      border-color: #232326;
    }
  }
}

.centered-dialog--hide-header {
  :deep(.el-dialog__header) {
    display: none;
  }
}
</style>
