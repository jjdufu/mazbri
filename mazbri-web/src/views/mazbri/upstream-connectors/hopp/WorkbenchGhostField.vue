<template>
  <label class="wb-ghost" @focusin="onSelectFocusIn" @focusout="onSelectFocusOut">
    <span class="wb-ghost__label" :title="label">{{ label }}</span>
    <div class="wb-ghost__control">
      <HoppAutoComplete
        v-if="kind === 'select'"
        :model-value="selectDraft"
        :options="options"
        :append-to="appendTo"
        :disabled="disabled"
        :placeholder="placeholder"
        list-all-when-exact
        @update:model-value="onSelectDraft"
        @select="onSelectPick"
      />
      <input
        v-else
        class="wb-ghost__input"
        :type="kind === 'number' ? 'number' : 'text'"
        :value="modelValue ?? ''"
        :placeholder="placeholder"
        :disabled="disabled"
        :min="min"
        :step="step"
        spellcheck="false"
        @input="onInput"
      />
      <slot />
    </div>
  </label>
</template>

<script setup lang="ts">
import HoppAutoComplete from "@/components/common/hopp-autocomplete/index.vue";
import type { HoppSelectOption } from "@/components/common/hopp-select/index.vue";

const props = withDefaults(
  defineProps<{
    label: string;
    modelValue?: string | number;
    kind?: "text" | "number" | "select";
    options?: HoppSelectOption[];
    placeholder?: string;
    disabled?: boolean;
    appendTo?: HTMLElement | string;
    min?: number;
    step?: number;
  }>(),
  {
    modelValue: "",
    kind: "text",
    options: () => [],
    placeholder: "",
    disabled: false,
    appendTo: undefined,
    min: undefined,
    step: undefined,
  },
);

const emit = defineEmits<{
  "update:modelValue": [string | number];
}>();

const selectDraft = ref("");
const selectFocused = ref(false);

function optionLabel(value: string): string {
  const hit = props.options.find((o) => o.value === value);
  return hit?.label || value;
}

function syncSelectDraft() {
  selectDraft.value = optionLabel(String(props.modelValue ?? ""));
}

watch(
  () => [props.modelValue, props.options, props.kind] as const,
  () => {
    if (props.kind !== "select" || selectFocused.value) return;
    syncSelectDraft();
  },
  { immediate: true },
);

function onSelectFocusIn() {
  if (props.kind === "select") selectFocused.value = true;
}

function onSelectFocusOut(ev: FocusEvent) {
  if (props.kind !== "select") return;
  const root = ev.currentTarget as HTMLElement | null;
  const next = ev.relatedTarget as Node | null;
  if (root && next && root.contains(next)) return;
  selectFocused.value = false;
  syncSelectDraft();
}

function onSelectDraft(raw: string) {
  selectDraft.value = raw;
  const hit = props.options.find((o) => o.label === raw || o.value === raw);
  if (hit) emit("update:modelValue", hit.value);
}

function onSelectPick(value: string) {
  emit("update:modelValue", value);
  selectDraft.value = optionLabel(value);
}

function onInput(ev: Event) {
  const raw = (ev.target as HTMLInputElement).value;
  if (props.kind === "number") {
    const n = Number(raw);
    emit("update:modelValue", Number.isFinite(n) ? n : 0);
    return;
  }
  emit("update:modelValue", raw);
}
</script>

<style scoped lang="scss">
.wb-ghost {
  display: flex;
  align-items: stretch;
  min-height: 32px;
  border-bottom: 1px solid var(--wb-divider-light, var(--el-border-color-extra-light));
}
.wb-ghost__label {
  box-sizing: border-box;
  flex: 0 0 var(--wb-label-col, 10.5rem);
  width: var(--wb-label-col, 10.5rem);
  display: block;
  padding: 0 12px 0 16px;
  font-size: 12px;
  font-weight: 600;
  color: var(--wb-text-muted, var(--el-text-color-secondary));
  line-height: 32px;
  white-space: nowrap;
  overflow: visible;
  text-overflow: clip;
}
.wb-ghost__control {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: stretch;
  gap: 8px;
}
.wb-ghost__control :deep(.hopp-ac-host) {
  flex: 1;
  min-width: 0;
  height: auto;
  align-self: stretch;
  border-bottom: 1px solid transparent;
}
.wb-ghost__control :deep(.hopp-ac__input) {
  min-height: 32px;
  padding: 0 8px;
}
.wb-ghost__input {
  flex: 1;
  min-width: 0;
  height: 32px;
  border: none;
  outline: none;
  background: transparent;
  padding: 0 8px;
  font-size: 13px;
  color: var(--wb-text, var(--el-text-color-primary));
  &::placeholder {
    color: var(--wb-text-faint, var(--el-text-color-placeholder));
  }
  &:disabled {
    opacity: 0.55;
  }
}
</style>
