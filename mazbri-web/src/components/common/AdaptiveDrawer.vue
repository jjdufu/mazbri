<template>
  <ElDrawer
    v-model="visible"
    :title="props.title"
    :direction="props.direction"
    :size="resolvedSize"
    :with-header="props.withHeader"
    :modal-fade="props.modalFade"
    :header-aria-level="props.headerAriaLevel"
    :append-to-body="props.appendToBody"
    :append-to="props.appendTo"
    :before-close="props.beforeClose"
    :destroy-on-close="props.destroyOnClose"
    :close-on-click-modal="props.closeOnClickModal"
    :close-on-press-escape="props.closeOnPressEscape"
    :lock-scroll="props.lockScroll"
    :modal="props.modal"
    :modal-penetrable="props.modalPenetrable"
    :open-delay="props.openDelay"
    :close-delay="props.closeDelay"
    :top="props.top"
    :modal-class="props.modalClass"
    :header-class="props.headerClass"
    :body-class="props.bodyClass"
    :footer-class="props.footerClass"
    :z-index="props.zIndex"
    :trap-focus="props.trapFocus"
    :transition="props.transition"
    :center="props.center"
    :align-center="props.alignCenter"
    :close-icon="props.closeIcon"
    :draggable="props.draggable"
    :overflow="props.overflow"
    :fullscreen="props.fullscreen"
    :show-close="props.showClose"
    :aria-level="props.ariaLevel"
    :class="[
      'adaptive-drawer',
      props.drawerClass,
      { 'adaptive-drawer--content-width': useContentWidth },
    ]"
    :style="drawerStyle"
    @open="emit('open')"
    @opened="emit('opened')"
    @close="emit('close')"
    @closed="emit('closed')"
    @update:model-value="emit('update:modelValue', $event)"
  >
    <template v-if="$slots.header" #header>
      <slot name="header" />
    </template>

    <slot />

    <template v-if="$slots.footer" #footer>
      <slot name="footer" />
    </template>
  </ElDrawer>
</template>

<script setup lang="ts">
interface Props {
  title?: string;
  size?: string | number;
  autoWidth?: boolean;
  minSize?: string | number;
  direction?: "ltr" | "rtl" | "ttb" | "btt";
  withHeader?: boolean;
  modalFade?: boolean;
  headerAriaLevel?: string;
  appendToBody?: boolean;
  appendTo?: string | HTMLElement;
  beforeClose?: (done: () => void) => void;
  destroyOnClose?: boolean;
  closeOnClickModal?: boolean;
  closeOnPressEscape?: boolean;
  lockScroll?: boolean;
  modal?: boolean;
  modalPenetrable?: boolean;
  openDelay?: number;
  closeDelay?: number;
  top?: string;
  modalClass?: string;
  headerClass?: string;
  bodyClass?: string;
  footerClass?: string;
  zIndex?: number;
  trapFocus?: boolean;
  transition?: string | Record<string, unknown>;
  center?: boolean;
  alignCenter?: boolean;
  closeIcon?: string | object;
  draggable?: boolean;
  overflow?: boolean;
  fullscreen?: boolean;
  showClose?: boolean;
  ariaLevel?: string;
  drawerClass?: string;
}
const props = withDefaults(defineProps<Props>(), {
  title: "",
  size: undefined,
  autoWidth: true,
  minSize: "min(90vw, 960px)",
  direction: "rtl",
  withHeader: true,
  modalFade: true,
  headerAriaLevel: "2",
  appendToBody: true,
  appendTo: "body",
  beforeClose: undefined,
  destroyOnClose: false,
  closeOnClickModal: true,
  closeOnPressEscape: true,
  lockScroll: true,
  modal: true,
  modalPenetrable: false,
  openDelay: 0,
  closeDelay: 0,
  top: "",
  modalClass: "",
  headerClass: "",
  bodyClass: "",
  footerClass: "",
  zIndex: undefined,
  trapFocus: true,
  transition: undefined,
  center: false,
  alignCenter: false,
  closeIcon: undefined,
  draggable: false,
  overflow: false,
  fullscreen: false,
  showClose: true,
  ariaLevel: "2",
  drawerClass: "",
});
const emit = defineEmits<{
  open: [];
  opened: [];
  close: [];
  closed: [];
  "update:modelValue": [value: boolean];
}>();
const visible = defineModel<boolean>({ required: true });
const useContentWidth = computed(
  () => props.size === undefined && props.autoWidth,
);
const resolvedSize = computed(() => {
  if (props.size !== undefined) {
    return props.size;
  }
  return props.autoWidth ? "fit-content" : props.minSize;
});
const drawerStyle = computed(() => {
  if (!useContentWidth.value) {
    return undefined;
  }
  const minSize =
    typeof props.minSize === "number" ? `${props.minSize}px` : props.minSize;
  return {
    "--adaptive-drawer-min-size": minSize,
  } as Record<string, string>;
});
</script>

<style scoped lang="scss">
.adaptive-drawer {
  max-width: calc(100vw - 32px);
}

.adaptive-drawer--content-width {
  min-width: var(--adaptive-drawer-min-size);
}

.adaptive-drawer :deep(.el-drawer__body) {
  box-sizing: border-box;
}
</style>
