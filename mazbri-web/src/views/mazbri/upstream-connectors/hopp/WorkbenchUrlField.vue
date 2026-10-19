<template>
  <div class="wb-url-field" role="textbox">
    <WorkbenchEnvText
      ref="envRef"
      compact
      :model-value="modelValue"
      :env="env"
      :placeholder="placeholder"
      @update:model-value="emit('update:modelValue', $event)"
      @focus="emit('focus', $event)"
      @blur="emit('blur', $event)"
      @keydown="onKeydown"
      @paste="onPaste"
    />
  </div>
</template>

<script setup lang="ts">
import type { ParsedImport } from "../workbenchHelpers";
import { tryParseCurlPaste } from "../workbenchHelpers";
import WorkbenchEnvText from "./WorkbenchEnvText.vue";

withDefaults(
  defineProps<{
    modelValue: string;
    env?: Record<string, string>;
    placeholder?: string;
  }>(),
  { env: () => ({}), placeholder: "" },
);

const emit = defineEmits<{
  "update:modelValue": [string];
  focus: [FocusEvent];
  blur: [FocusEvent];
  enter: [];
  curl: [ParsedImport];
}>();

function onKeydown(ev: KeyboardEvent) {
  if (ev.key === "Enter") {
    ev.preventDefault();
    emit("enter");
  }
}

const envRef = ref<{ focus: () => void; blur: () => void } | null>(null);

defineExpose({
  focus: () => envRef.value?.focus(),
  blur: () => envRef.value?.blur(),
});

function onPaste(ev: ClipboardEvent) {
  const text = ev.clipboardData?.getData("text") ?? "";
  const parsed = tryParseCurlPaste(text);
  if (!parsed) return;
  ev.preventDefault();
  emit("curl", parsed);
}
</script>

<style scoped lang="scss">
.wb-url-field {
  flex: 1;
  min-width: 0;
  height: 100%;
  border-left: 1px solid var(--wb-divider-subtle, var(--el-border-color-extra-light));
}
</style>
