<template>
  <ElPopover
    v-model:visible="open"
    :trigger="trigger"
    :show-arrow="false"
    effect="light"
    :teleported="true"
    :append-to="appendTo || undefined"
    :placement="placement"
    :gpu-acceleration="false"
    transition="hopp-scale-subtle"
    :offset="offset"
    width="auto"
    :show-after="0"
    :hide-after="0"
    :disabled="disabled"
    :popper-class="popperClass"
  >
    <template #reference>
      <slot name="trigger" />
    </template>
    <div
      ref="menuEl"
      class="hopp-menu"
      tabindex="0"
      role="menu"
      @keydown.esc.stop="open = false"
      @click="onMenuClick"
    >
      <slot />
    </div>
  </ElPopover>
</template>

<script setup lang="ts">
defineOptions({ name: "HoppMenu" });

export type HoppMenuPlacement = "bottom-start" | "bottom-end";

const props = withDefaults(
  defineProps<{
    visible?: boolean;
    placement?: HoppMenuPlacement;
    appendTo?: HTMLElement | string;
    disabled?: boolean;
    offset?: number;
    trigger?:
      | "click"
      | "hover"
      | "focus"
      | "contextmenu"
      | Array<"click" | "hover" | "focus" | "contextmenu">;
  }>(),
  {
    visible: undefined,
    placement: "bottom-start",
    appendTo: undefined,
    disabled: false,
    offset: 10,
    trigger: "click",
  },
);

const emit = defineEmits<{
  "update:visible": [boolean];
}>();

const internal = ref(false);
const menuEl = ref<HTMLElement | null>(null);
const controlled = computed(() => props.visible !== undefined);

const open = computed({
  get: () => (controlled.value ? Boolean(props.visible) : internal.value),
  set: (v: boolean) => {
    if (!controlled.value) internal.value = v;
    emit("update:visible", v);
  },
});

const popperClass = computed(() =>
  [
    "hopp-menu-popper",
    props.placement === "bottom-end"
      ? "hopp-menu-popper--end"
      : "hopp-menu-popper--start",
  ].join(" "),
);

watch(open, async (v) => {
  if (!v) return;
  await nextTick();
  menuEl.value?.focus();
});

function onMenuClick(e: MouseEvent) {
  const t = e.target;
  if (!(t instanceof Element) || !t.closest(".hopp-menu-item")) return;
  open.value = false;
}
</script>

<style lang="scss">
.el-popper.hopp-menu-popper {
  --hopp-menu-bg: #ffffff;
  --hopp-menu-border: #d1d5db;
  padding: 8px !important;
  margin: 0 !important;
  background: var(--hopp-menu-bg) !important;
  border: 1px solid var(--hopp-menu-border) !important;
  border-radius: var(--art-form-control-radius, 4px) !important;
  box-shadow: none !important;
  filter: drop-shadow(0 8px 12px rgb(0 0 0 / 12%));
  min-width: 0 !important;
  width: max-content !important;
  overflow: visible !important;
  transform-origin: 22px top;
}

html.dark .el-popper.hopp-menu-popper {
  --hopp-menu-bg: #1b1b1b;
  --hopp-menu-border: #3f3f46;
  filter: drop-shadow(0 10px 16px rgb(0 0 0 / 45%));
}

.el-popper.hopp-menu-popper--end {
  transform-origin: calc(100% - 22px) top;
}

.el-popper.hopp-menu-popper > .el-popper__arrow {
  display: none !important;
}

.el-popper.hopp-menu-popper::before,
.el-popper.hopp-menu-popper::after {
  content: "";
  position: absolute;
  pointer-events: none;
  width: 0;
  height: 0;
  border-style: solid;
  border-color: transparent;
}

.el-popper.hopp-menu-popper::before {
  left: 20px;
  top: -8px;
  border-width: 0 8px 8px;
  border-bottom-color: var(--hopp-menu-border);
  z-index: 3;
}

.el-popper.hopp-menu-popper::after {
  left: 21px;
  top: -7px;
  border-width: 0 7px 8px;
  border-bottom-color: var(--hopp-menu-bg);
  z-index: 4;
}

.el-popper.hopp-menu-popper--end::before {
  left: auto;
  right: 12px;
}

.el-popper.hopp-menu-popper--end::after {
  left: auto;
  right: 13px;
}

.hopp-scale-subtle-enter-active,
.hopp-scale-subtle-leave-active {
  transition:
    transform 200ms cubic-bezier(0.16, 1, 0.3, 1),
    opacity 200ms ease !important;
}

.hopp-scale-subtle-enter-from,
.hopp-scale-subtle-leave-to {
  opacity: 0 !important;
  transform: scale(0.8) !important;
}

.hopp-scale-subtle-enter-to,
.hopp-scale-subtle-leave-from {
  opacity: 1 !important;
  transform: scale(1) !important;
}

html.radius-zero .el-popper.hopp-menu-popper,
html.radius-zero .hopp-menu-item {
  border-radius: 0 !important;
}

.hopp-menu {
  display: flex;
  flex-direction: column;
  outline: none;
}

.hopp-menu-item {
  display: block;
  width: 100%;
  border: none;
  background: transparent;
  text-align: left;
  padding: 6px 12px;
  font-size: 12px;
  font-weight: 600;
  line-height: 1rem;
  color: var(--el-text-color-primary);
  cursor: pointer;
  border-radius: var(--art-form-control-radius, 4px);
  white-space: nowrap;
  transition: background-color 0.15s ease;
}

.hopp-menu-item:hover {
  background: rgb(0 0 0 / 4%);
}

html.dark .hopp-menu-item:hover {
  background: rgb(255 255 255 / 6%);
}

.hopp-menu-item.is-current {
  font-weight: 700;
}

.hopp-menu-item.is-danger {
  color: var(--el-color-danger);
}

.hopp-menu-item.is-divided {
  margin-top: 8px;
  border-top: 1px solid var(--el-border-color-extra-light);
  border-radius: 0 0 var(--art-form-control-radius, 4px)
    var(--art-form-control-radius, 4px);
}

.hopp-menu-caret {
  width: 6px;
  height: 6px;
  flex-shrink: 0;
  display: inline-block;
  border-right: 1.5px solid currentColor;
  border-bottom: 1.5px solid currentColor;
  transform: rotate(45deg) translateY(-1px);
  opacity: 0.65;
  pointer-events: none;
}
</style>
