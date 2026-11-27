<template>
  <HoppMenu
    v-model:visible="open"
    :append-to="appendTo"
    :placement="placement"
    :disabled="disabled"
  >
    <template #trigger>
      <span
        class="hopp-select-wrap"
        :class="{ 'is-block': block, 'is-sized': Boolean(width || block) }"
        :style="width ? { width } : undefined"
      >
        <button
          type="button"
          class="hopp-select-trigger"
          :class="{ 'is-open': open, 'is-block': block }"
          :disabled="disabled"
          :title="title"
        >
          <span class="hopp-select-trigger__label">{{ displayLabel }}</span>
          <span class="hopp-menu-caret" aria-hidden="true" />
        </button>
      </span>
    </template>
    <button
      v-for="opt in options"
      :key="String(opt.value)"
      type="button"
      class="hopp-menu-item"
      role="option"
      :aria-selected="opt.value === modelValue"
      :class="{
        'is-current': opt.value === modelValue,
        'is-divided': opt.divided,
        'is-danger': opt.danger,
      }"
      :style="opt.color ? { color: opt.color } : undefined"
      @click="pick(opt.value)"
    >
      {{ opt.label }}
    </button>
  </HoppMenu>
</template>

<script setup lang="ts">
import HoppMenu from "@/components/common/hopp-menu/index.vue";
import type { HoppMenuPlacement } from "@/components/common/hopp-menu/index.vue";

defineOptions({ name: "HoppSelect" });

export type HoppSelectOption = {
  label: string;
  value: string;
  color?: string;
  divided?: boolean;
  danger?: boolean;
};

const props = withDefaults(
  defineProps<{
    modelValue: string;
    options: HoppSelectOption[];
    appendTo?: HTMLElement | string;
    disabled?: boolean;
    title?: string;
    placeholder?: string;
    placement?: HoppMenuPlacement;
    block?: boolean;
    width?: string;
  }>(),
  {
    appendTo: undefined,
    disabled: false,
    title: "",
    placeholder: "",
    placement: "bottom-start",
    block: false,
    width: "",
  },
);

const emit = defineEmits<{
  "update:modelValue": [string];
}>();

const open = ref(false);

const displayLabel = computed(() => {
  const hit = props.options.find((o) => o.value === props.modelValue);
  return hit?.label || props.placeholder || props.modelValue || "";
});

function pick(value: string) {
  emit("update:modelValue", value);
  open.value = false;
}
</script>

<style scoped lang="scss">
.hopp-select-wrap {
  display: inline-flex;
  max-width: 100%;
  vertical-align: middle;
  &.is-block {
    display: flex;
    width: 100%;
  }
  &.is-sized .hopp-select-trigger {
    width: 100%;
    justify-content: space-between;
  }
}
.hopp-select-trigger {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-height: 32px;
  border: none;
  background: transparent;
  color: var(--el-text-color-secondary);
  font-size: 13px;
  font-weight: 600;
  padding: 0 8px;
  cursor: pointer;
  border-radius: 0;
  transition: color 0.15s ease;
  &:hover:not(:disabled),
  &.is-open:not(:disabled) {
    background: transparent;
    color: var(--el-text-color-primary);
  }
  &:disabled {
    cursor: default;
    opacity: 0.55;
  }
  &.is-block {
    width: 100%;
    justify-content: space-between;
  }
}
.hopp-select-trigger__label {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.hopp-select-trigger :deep(.hopp-menu-caret) {
  color: currentColor;
  opacity: 0.65;
}
.hopp-select-trigger:hover:not(:disabled) :deep(.hopp-menu-caret),
.hopp-select-trigger.is-open :deep(.hopp-menu-caret) {
  color: currentColor;
  opacity: 1;
}
</style>
