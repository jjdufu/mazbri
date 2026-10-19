<template>
  <div class="wb-env-wrap">
  <HoppMenu
    :visible="suggestOpen"
    :trigger="[]"
    :append-to="suggestAppendTo"
    placement="bottom-start"
  >
    <template #trigger>
      <div
        class="wb-env"
        :class="{ 'is-empty': !modelValue, 'is-compact': compact, 'is-secret': secret }"
      >
        <div ref="overlayEl" class="wb-env__overlay" aria-hidden="true">
          <span class="wb-env__track">
            <template v-for="(seg, i) in segments" :key="i">
              <span
                v-if="seg.kind === 'var'"
                :class="seg.found ? 'wb-env-ok' : 'wb-env-miss'"
                >{{ seg.raw }}</span
              >
              <span v-else>{{ seg.text }}</span>
            </template>
          </span>
        </div>
        <input
          ref="inputEl"
          class="wb-env__input"
          :type="secret ? 'password' : 'text'"
          :value="modelValue"
          :placeholder="placeholder"
          :disabled="disabled"
          :list="list"
          :spellcheck="false"
          autocomplete="off"
          @input="onInput"
          @scroll="syncScroll"
          @focus="onFocus($event)"
          @blur="onBlur($event)"
          @keydown="onKeydown"
          @keyup="refreshSuggest"
          @click="refreshSuggest"
          @paste="emit('paste', $event)"
          @compositionstart="composing = true"
          @compositionend="onCompositionEnd"
        />
      </div>
    </template>
    <button
      v-for="(k, i) in suggestions"
      :key="k"
      type="button"
      class="hopp-menu-item wb-env-suggest"
      :class="{ 'is-current': i === activeIdx }"
      @mousedown.prevent
      @click="pick(k)"
    >
      <span>{{ k }}</span>
      <span class="wb-env-suggest__val">{{ env[k] }}</span>
    </button>
  </HoppMenu>
  </div>
</template>

<script setup lang="ts">
import HoppMenu from "@/components/common/hopp-menu/index.vue";
import {
  applyEnvVarCompletion,
  filterEnvKeys,
  incompleteEnvVar,
  splitEnvText,
} from "../workbenchHelpers";

const props = withDefaults(
  defineProps<{
    modelValue: string;
    env?: Record<string, string>;
    placeholder?: string;
    disabled?: boolean;
    list?: string;
    compact?: boolean;
    secret?: boolean;
  }>(),
  {
    env: () => ({}),
    placeholder: "",
    disabled: false,
    list: undefined,
    compact: false,
    secret: false,
  },
);

const emit = defineEmits<{
  "update:modelValue": [string];
  focus: [FocusEvent];
  blur: [FocusEvent];
  keydown: [KeyboardEvent];
  paste: [ClipboardEvent];
}>();

const inputEl = ref<HTMLInputElement | null>(null);
const overlayEl = ref<HTMLElement | null>(null);
const composing = ref(false);
const suggestions = ref<string[]>([]);
const activeIdx = ref(0);
const suggestAppendTo = ref<HTMLElement | string | undefined>(undefined);

const suggestOpen = computed(() => suggestions.value.length > 0);

const segments = computed(() => {
  if (composing.value) {
    return props.modelValue ? [{ kind: "text" as const, text: props.modelValue }] : [];
  }
  return splitEnvText(props.modelValue || "", props.env);
});

function onInput(ev: Event) {
  const el = ev.target as HTMLInputElement;
  emit("update:modelValue", el.value);
  syncScroll();
  if (!composing.value) updateSuggest(el);
}

function onFocus(ev: FocusEvent) {
  emit("focus", ev);
  refreshSuggest();
}

function onBlur(ev: FocusEvent) {
  suggestions.value = [];
  emit("blur", ev);
}

function onCompositionEnd(ev: CompositionEvent) {
  composing.value = false;
  const el = ev.target as HTMLInputElement;
  emit("update:modelValue", el.value);
  updateSuggest(el);
}

function onKeydown(ev: KeyboardEvent) {
  if (suggestOpen.value) {
    if (ev.key === "ArrowDown") {
      ev.preventDefault();
      activeIdx.value = (activeIdx.value + 1) % suggestions.value.length;
      return;
    }
    if (ev.key === "ArrowUp") {
      ev.preventDefault();
      activeIdx.value =
        (activeIdx.value - 1 + suggestions.value.length) % suggestions.value.length;
      return;
    }
    if (ev.key === "Enter" || ev.key === "Tab") {
      const name = suggestions.value[activeIdx.value];
      if (name) {
        ev.preventDefault();
        pick(name);
        return;
      }
    }
    if (ev.key === "Escape") {
      ev.preventDefault();
      suggestions.value = [];
      return;
    }
  }
  emit("keydown", ev);
}

function refreshSuggest() {
  const el = inputEl.value;
  if (el && !composing.value) updateSuggest(el);
}

function updateSuggest(el: HTMLInputElement) {
  const caret = el.selectionStart ?? el.value.length;
  const hit = incompleteEnvVar(el.value, caret);
  if (!hit) {
    suggestions.value = [];
    return;
  }
  const keys = filterEnvKeys(props.env, hit.prefix);
  suggestions.value = keys;
  activeIdx.value = 0;
  const page = el.closest(".connector-workbench-page");
  suggestAppendTo.value =
    page instanceof HTMLElement ? page : document.body;
}

function pick(name: string) {
  const el = inputEl.value;
  const caret = el?.selectionStart ?? (props.modelValue || "").length;
  const next = applyEnvVarCompletion(props.modelValue || "", caret, name);
  if (!next) return;
  emit("update:modelValue", next.text);
  suggestions.value = [];
  void nextTick(() => {
    const input = inputEl.value;
    if (!input) return;
    input.focus();
    input.setSelectionRange(next.caret, next.caret);
    syncScroll();
  });
}

function syncScroll() {
  const overlay = overlayEl.value;
  const input = inputEl.value;
  if (!overlay || !input) return;
  overlay.scrollLeft = input.scrollLeft;
}

watch(
  () => props.modelValue,
  () => {
    void nextTick(syncScroll);
  },
);

defineExpose({
  focus: () => inputEl.value?.focus(),
  blur: () => inputEl.value?.blur(),
});
</script>

<style scoped lang="scss">
.wb-env-wrap {
  width: 100%;
  height: 100%;
  min-width: 0;
  :deep(.el-tooltip__trigger) {
    display: block;
    width: 100%;
    height: 100%;
  }
}
.wb-env {
  position: relative;
  min-width: 0;
  width: 100%;
  height: 100%;
}
.wb-env__overlay,
.wb-env__input {
  font-size: 13px;
  line-height: 38px;
  padding: 0 12px;
  white-space: nowrap;
}
.wb-env__overlay {
  position: absolute;
  inset: 0;
  overflow: hidden;
  pointer-events: none;
  color: var(--wb-text, var(--el-text-color-primary));
}
.wb-env__track {
  display: inline-block;
  white-space: pre;
}
.wb-env__input {
  position: relative;
  z-index: 1;
  width: 100%;
  height: 100%;
  min-height: 38px;
  border: none;
  outline: none;
  background: transparent;
  color: transparent;
  caret-color: var(--wb-text, var(--el-text-color-primary));
  &::placeholder {
    color: var(--wb-text-faint, var(--el-text-color-placeholder));
  }
}
.wb-env.is-empty .wb-env__input {
  color: var(--wb-text, var(--el-text-color-primary));
}
.wb-env.is-compact .wb-env__overlay,
.wb-env.is-compact .wb-env__input {
  font-size: var(--wb-fs-url, 13px);
  line-height: var(--wb-urlbar-h, 32px);
  padding: 0 16px;
}
.wb-env.is-compact .wb-env__input {
  min-height: 0;
  &::placeholder {
    color: var(--wb-text-muted, var(--el-text-color-secondary));
  }
}
.wb-env.is-secret .wb-env__overlay {
  display: none;
}
.wb-env.is-secret .wb-env__input {
  color: var(--wb-text, var(--el-text-color-primary));
}
.wb-env-ok {
  background: var(--wb-env-ok-bg, #22c55e);
  color: #fff;
  border-radius: var(--art-compact-radius);
  padding: 0 4px;
}
.wb-env-miss {
  background: var(--el-color-danger);
  color: #fff;
  border-radius: var(--art-compact-radius);
  padding: 0 4px;
}
.wb-env-suggest {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}
.wb-env-suggest__val {
  color: var(--el-text-color-placeholder);
  font-size: 12px;
  max-width: 10rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
