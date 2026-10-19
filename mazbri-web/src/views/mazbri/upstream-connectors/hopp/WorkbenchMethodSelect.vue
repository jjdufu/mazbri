<template>
  <div
    class="wb-method"
    :class="{ 'is-open': open, 'is-custom': isCustomMethod }"
  >
    <input
      v-if="isCustomMethod"
      ref="inputEl"
      class="wb-method__input"
      :value="customDraft"
      maxlength="16"
      spellcheck="false"
      autocomplete="off"
      :style="{ color: methodColorVar(customDraft || 'CUSTOM') }"
      @input="onTyped"
    />
    <HoppMenu
      v-model:visible="open"
      :append-to="appendTo"
      placement="bottom-start"
    >
      <template #trigger>
        <button
          type="button"
          class="wb-method__btn"
          :aria-expanded="open"
          aria-haspopup="listbox"
        >
          <span
            v-if="!isCustomMethod"
            class="wb-method__label"
            :style="{ color: methodColorVar(modelValue) }"
            >{{ shownMethod }}</span
          >
          <span class="hopp-menu-caret" aria-hidden="true" />
        </button>
      </template>
      <button
        v-for="m in HTTP_METHODS"
        :key="m"
        type="button"
        class="hopp-menu-item"
        role="option"
        :aria-selected="m === shownMethod"
        :class="{ 'is-current': m === shownMethod }"
        :style="{ color: methodColorVar(m) }"
        @click="pick(m)"
      >
        {{ m }}
      </button>
    </HoppMenu>
  </div>
</template>

<script setup lang="ts">
import HoppMenu from "@/components/common/hopp-menu/index.vue";
import {
  HTTP_METHOD_PRESETS,
  HTTP_METHODS,
  displayMethodBadge,
  methodColorVar,
  sanitizeHttpMethod,
} from "../workbenchHelpers";

const props = withDefaults(
  defineProps<{
    modelValue: string;
    appendTo?: HTMLElement | string;
  }>(),
  { appendTo: undefined },
);

const emit = defineEmits<{
  "update:modelValue": [string];
  "open-change": [boolean];
}>();

const open = ref(false);
const inputEl = ref<HTMLInputElement | null>(null);
const customDraft = ref("");

const shownMethod = computed(() => displayMethodBadge(props.modelValue));

const isCustomMethod = computed(() => {
  const m = (props.modelValue || "GET").toUpperCase();
  return m === "CUSTOM" || !(HTTP_METHOD_PRESETS as readonly string[]).includes(m);
});

watch(
  () => props.modelValue,
  (v) => {
    if (isCustomMethod.value) customDraft.value = (v || "").toUpperCase().slice(0, 16);
  },
  { immediate: true },
);

watch(open, (v) => emit("open-change", v));

function onTyped(e: Event) {
  const el = e.target as HTMLInputElement;
  const raw = el.value.toUpperCase().slice(0, 16);
  customDraft.value = raw;
  el.value = raw;
  if (!raw) return;
  const next = sanitizeHttpMethod(raw);
  if (next) emit("update:modelValue", next);
}

function pick(method: string) {
  if (method === "CUSTOM") {
    emit("update:modelValue", "CUSTOM");
    customDraft.value = "";
    open.value = false;
    void nextTick(() => inputEl.value?.focus());
    return;
  }
  emit("update:modelValue", method);
  open.value = false;
}
</script>

<style scoped lang="scss">
.wb-method {
  display: inline-flex;
  align-items: center;
  width: 6.5rem;
  flex-shrink: 0;
  height: var(--wb-urlbar-h, 32px);
  padding: 0 4px 0 12px;
  outline: none;
}
.wb-method__btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 100%;
  min-width: 0;
  flex: 1;
  border: none;
  background: transparent;
  padding: 0 4px 0 0;
  cursor: pointer;
  color: inherit;
}
.wb-method__label {
  flex: 1;
  min-width: 0;
  font-size: 13px;
  font-weight: 600;
  line-height: 1;
  text-align: left;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.wb-method__input {
  flex: 1;
  min-width: 0;
  height: 100%;
  border: none;
  background: transparent;
  padding: 0 4px 0 0;
  font-size: 13px;
  font-weight: 600;
  line-height: 1;
  outline: none;
  box-shadow: none;
  appearance: none;
  color: inherit;
}
.wb-method :deep(.hopp-menu-caret) {
  margin-right: 4px;
  color: var(--wb-text-muted, var(--el-text-color-secondary));
  opacity: 1;
}
.wb-method.is-open :deep(.hopp-menu-caret) {
  color: var(--wb-text, var(--el-text-color-primary));
}
</style>
