<template>
  <CenteredDialog
    v-model="visible"
    :title="title"
    :width="width"
    :show-close="showClose"
    :close-on-click-modal="closeOnClickModal"
    :close-on-press-escape="closeOnPressEscape"
    :destroy-on-close="destroyOnClose"
    :lock-scroll="lockScroll"
    :append-to-body="appendToBody"
    :modal-class="modalClass"
    :dialog-class="dialogClass"
    @open="emit('open')"
    @opened="emit('opened')"
    @close="emit('close')"
    @closed="emit('closed')"
  >
    <div :class="['edit-dialog__body', bodyClass]">
      <slot />
    </div>

    <template #footer>
      <div class="edit-dialog__footer">
        <slot name="footer">
          <ElButton class="btn-cancel" @click="handleCancel">{{
            resolvedCancelText
          }}</ElButton>
          <ElTooltip
            :disabled="!confirmDisabled || !confirmDisabledReason"
            :content="confirmDisabledReason"
          >
            <span>
              <ElButton
                class="btn-confirm"
                :type="confirmType"
                :loading="saving"
                :disabled="confirmDisabled"
                @click="emit('confirm')"
              >
                {{ resolvedConfirmText }}
              </ElButton>
            </span>
          </ElTooltip>
        </slot>
      </div>
    </template>
  </CenteredDialog>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import CenteredDialog from "./CenteredDialog.vue";
interface Props {
  title: string;
  width?: string | number;
  saving?: boolean;
  confirmText?: string;
  confirmType?: "primary" | "success" | "warning" | "danger" | "info";
  confirmDisabled?: boolean;
  confirmDisabledReason?: string;
  cancelText?: string;
  showClose?: boolean;
  closeOnClickModal?: boolean;
  closeOnPressEscape?: boolean;
  destroyOnClose?: boolean;
  lockScroll?: boolean;
  appendToBody?: boolean;
  modalClass?: string;
  dialogClass?: string;
  bodyClass?: string;
}
const props = withDefaults(defineProps<Props>(), {
  width: "560px",
  saving: false,
  confirmText: undefined,
  confirmType: "primary",
  confirmDisabled: false,
  confirmDisabledReason: undefined,
  cancelText: undefined,
  showClose: true,
  closeOnClickModal: false,
  closeOnPressEscape: true,
  destroyOnClose: true,
  lockScroll: true,
  appendToBody: true,
  modalClass: "",
  dialogClass: "",
  bodyClass: "",
});
const emit = defineEmits<{
  confirm: [];
  cancel: [];
  open: [];
  opened: [];
  close: [];
  closed: [];
}>();
const visible = defineModel<boolean>({ required: true });
const { t } = useI18n();
const resolvedConfirmText = computed(
  () => props.confirmText ?? t("common.actions.save"),
);
const resolvedCancelText = computed(
  () => props.cancelText ?? t("common.cancel"),
);
const handleCancel = () => {
  emit("cancel");
  visible.value = false;
};
const {
  title,
  width,
  saving,
  confirmType,
  confirmDisabled,
  confirmDisabledReason,
  showClose,
  closeOnClickModal,
  closeOnPressEscape,
  destroyOnClose,
  lockScroll,
  appendToBody,
  modalClass,
  dialogClass,
  bodyClass,
} = toRefs(props);
</script>

<style scoped lang="scss">
.edit-dialog__body {
  width: 100%;
}

:deep(.el-dialog__body) {
  max-height: calc(100vh - 180px);
  overflow-y: auto;
}

.edit-dialog__footer {
  display: flex;
  justify-content: flex-end;
  gap: 12px;
}
</style>
