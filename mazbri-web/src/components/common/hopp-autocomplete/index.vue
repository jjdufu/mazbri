<template>
  <div
    ref="hostEl"
    class="hopp-ac-host"
    :class="{ 'is-open': wantOpen }"
  >
    <input
      ref="inputEl"
      class="hopp-ac__input"
      :class="inputClass"
      :value="modelValue"
      :placeholder="placeholder"
      :disabled="disabled"
      :spellcheck="false"
      autocomplete="off"
      role="combobox"
      aria-autocomplete="list"
      :aria-expanded="wantOpen"
      :aria-controls="listId"
      :aria-activedescendant="activeOptionId"
      @input="onInput"
      @focus="onFocus"
      @blur="onBlur"
      @pointerdown="onPointerDown"
      @keydown="onKeydown"
    />
    <Teleport :to="teleportTo">
      <div
        v-show="wantOpen"
        :id="listId"
        ref="listEl"
        class="hopp-ac-popper"
        :class="{ 'is-scrolling': listScrolling }"
        :style="panelStyle"
        role="listbox"
        @mousedown.prevent
        @scroll="onListScroll"
      >
        <button
          v-for="(opt, i) in suggestions"
          :id="optionId(i)"
          :key="`${hoppAutoCompleteValue(opt)}-${i}`"
          type="button"
          class="hopp-ac-item"
          role="option"
          :aria-selected="i === activeIdx"
          :class="{ 'is-current': i === activeIdx }"
          @click="pick(opt)"
        >
          {{ hoppAutoCompleteLabel(opt) }}
        </button>
      </div>
    </Teleport>
  </div>
</template>

<script lang="ts">
export type { HoppAutoCompleteOption } from "./filter";
export {
  filterHoppAutoCompleteOptions,
  hoppAutoCompleteLabel,
  hoppAutoCompleteValue,
} from "./filter";
</script>

<script setup lang="ts">
import {
  filterHoppAutoCompleteOptions,
  hoppAutoCompleteLabel,
  hoppAutoCompleteValue,
  type HoppAutoCompleteOption,
} from "./filter";
import { hoppAcAnchorEl, hoppAcPanelRect } from "./geometry";

defineOptions({ name: "HoppAutoComplete" });

const ITEM_H = 40;
const VISIBLE = 5;
const LIST_PAD_Y = 4;

const props = withDefaults(
  defineProps<{
    modelValue: string;
    options: HoppAutoCompleteOption[];
    placeholder?: string;
    disabled?: boolean;
    inputClass?: string | string[] | Record<string, boolean>;
    appendTo?: string | HTMLElement;
    listAllWhenExact?: boolean;
  }>(),
  {
    placeholder: "",
    disabled: false,
    inputClass: undefined,
    appendTo: undefined,
    listAllWhenExact: false,
  },
);

const emit = defineEmits<{
  "update:modelValue": [string];
  select: [string];
}>();

const hostEl = ref<HTMLElement | null>(null);
const inputEl = ref<HTMLInputElement | null>(null);
const listEl = ref<HTMLElement | null>(null);
const focused = ref(false);
const dismissed = ref(false);
const activeIdx = ref(-1);
const listScrolling = ref(false);
const panel = ref({ top: 0, left: 0, width: 0 });
const uid = useId();
const listId = computed(() => `hopp-ac-list-${uid}`);
let scrollHideTimer = 0;

const teleportTo = computed(() => props.appendTo || "body");

const suggestions = computed(() => {
  const raw = props.modelValue || "";
  if (props.listAllWhenExact) {
    const q = raw.trim().toLowerCase();
    const exact =
      Boolean(q) &&
      props.options.some((opt) => {
        return (
          hoppAutoCompleteLabel(opt).toLowerCase() === q ||
          hoppAutoCompleteValue(opt).toLowerCase() === q
        );
      });
    if (exact) return [...props.options];
  }
  return filterHoppAutoCompleteOptions(props.options, raw);
});

const wantOpen = computed(
  () =>
    focused.value &&
    !dismissed.value &&
    !props.disabled &&
    suggestions.value.length > 0,
);

const panelStyle = computed(() => ({
  top: `${panel.value.top}px`,
  left: `${panel.value.left}px`,
  width: `${panel.value.width}px`,
  maxHeight: `${ITEM_H * VISIBLE + LIST_PAD_Y * 2}px`,
}));

const activeOptionId = computed(() =>
  wantOpen.value && activeIdx.value >= 0
    ? optionId(activeIdx.value)
    : undefined,
);

function optionId(i: number) {
  return `${listId.value}-opt-${i}`;
}

function measurePanel() {
  const el = hostEl.value;
  if (!el) return;
  const r = hoppAcAnchorEl(el).getBoundingClientRect();
  panel.value = hoppAcPanelRect(r);
}

function prepareOpen() {
  if (props.disabled) return;
  dismissed.value = false;
  measurePanel();
}

function onPointerDown() {
  prepareOpen();
}

function onInput(ev: Event) {
  dismissed.value = false;
  emit("update:modelValue", (ev.target as HTMLInputElement).value);
}

function onFocus() {
  prepareOpen();
  focused.value = true;
}

function onBlur() {
  focused.value = false;
}

function pick(opt: HoppAutoCompleteOption, restoreFocus = true) {
  const value = hoppAutoCompleteValue(opt);
  emit("update:modelValue", value);
  emit("select", value);
  dismissed.value = true;
  if (restoreFocus) {
    void nextTick(() => inputEl.value?.focus());
  }
}

function onListScroll() {
  listScrolling.value = true;
  window.clearTimeout(scrollHideTimer);
  scrollHideTimer = window.setTimeout(() => {
    listScrolling.value = false;
  }, 700);
}

function onKeydown(ev: KeyboardEvent) {
  if (ev.key === "Escape") {
    if (wantOpen.value) {
      ev.preventDefault();
      dismissed.value = true;
    }
    return;
  }
  if (ev.key === "ArrowDown") {
    ev.preventDefault();
    dismissed.value = false;
    if (!suggestions.value.length) return;
    const last = suggestions.value.length - 1;
    activeIdx.value =
      activeIdx.value < 0 ? 0 : Math.min(last, activeIdx.value + 1);
    return;
  }
  if (ev.key === "ArrowUp") {
    if (!wantOpen.value || !suggestions.value.length) return;
    ev.preventDefault();
    activeIdx.value =
      activeIdx.value <= 0 ? suggestions.value.length - 1 : activeIdx.value - 1;
    return;
  }
  if (ev.key === "Enter") {
    if (!wantOpen.value || activeIdx.value < 0) return;
    const opt = suggestions.value[activeIdx.value];
    if (!opt) return;
    ev.preventDefault();
    pick(opt);
    return;
  }
  if (ev.key === "Tab" && wantOpen.value && activeIdx.value >= 0) {
    const opt = suggestions.value[activeIdx.value];
    if (opt) pick(opt, false);
  }
}

function onWinChange() {
  if (wantOpen.value) measurePanel();
}

watch(suggestions, () => {
  activeIdx.value = -1;
});

watch(activeIdx, async () => {
  if (!wantOpen.value || activeIdx.value < 0) return;
  await nextTick();
  const cur = listEl.value?.querySelector(".is-current");
  if (cur instanceof HTMLElement) cur.scrollIntoView({ block: "nearest" });
});

watch(wantOpen, (v) => {
  if (v) measurePanel();
});

onMounted(() => {
  measurePanel();
  window.addEventListener("resize", onWinChange);
  window.addEventListener("scroll", onWinChange, true);
});

onBeforeUnmount(() => {
  window.clearTimeout(scrollHideTimer);
  window.removeEventListener("resize", onWinChange);
  window.removeEventListener("scroll", onWinChange, true);
});

defineExpose({
  focus: () => inputEl.value?.focus(),
  blur: () => inputEl.value?.blur(),
});
</script>

<style scoped lang="scss">
.hopp-ac-host {
  display: block;
  width: 100%;
  height: 100%;
  min-width: 0;
}
.hopp-ac-host.is-open {
  position: relative;
  z-index: 3101;
  background: #ffffff;
  border-bottom-color: #d1d5db !important;
}
.hopp-ac__input {
  width: 100%;
  height: 100%;
  min-height: 32px;
  border: none;
  outline: none;
  background: transparent;
  padding: 0 12px;
  font-size: 13px;
  color: var(--el-text-color-primary);
  &::placeholder {
    color: var(--el-text-color-placeholder);
  }
  &:disabled {
    opacity: 0.55;
    cursor: default;
  }
}
</style>

<style lang="scss">
.hopp-ac-popper {
  --hopp-ac-bg: #ffffff;
  --hopp-ac-border: #d1d5db;
  position: fixed;
  z-index: 3100;
  box-sizing: border-box;
  margin: 0;
  padding: 4px 0;
  overflow-x: hidden;
  overflow-y: auto;
  background: var(--hopp-ac-bg);
  border: 1px solid var(--hopp-ac-border);
  border-top: none;
  border-radius: 0 0 var(--art-form-control-radius, 4px)
    var(--art-form-control-radius, 4px);
  box-shadow: 0 4px 10px rgb(0 0 0 / 8%);
  outline: none;
  scrollbar-width: thin;
  scrollbar-color: transparent transparent;
}

html.dark .hopp-ac-popper {
  --hopp-ac-bg: #1b1b1b;
  --hopp-ac-border: #3f3f46;
  box-shadow: 0 6px 12px rgb(0 0 0 / 35%);
}

html.dark .hopp-ac-host.is-open {
  background: #1b1b1b;
  border-bottom-color: #3f3f46 !important;
}

html.radius-zero .hopp-ac-popper,
html.radius-zero .hopp-ac-item {
  border-radius: 0;
}

.hopp-ac-popper:hover,
.hopp-ac-popper.is-scrolling {
  scrollbar-color: var(--el-border-color) transparent;
}

.hopp-ac-popper::-webkit-scrollbar {
  width: 3px !important;
  height: 3px !important;
}

.hopp-ac-popper::-webkit-scrollbar-track {
  background: transparent !important;
}

.hopp-ac-popper::-webkit-scrollbar-thumb {
  background: transparent !important;
  border-radius: 999px;
  border: none;
}

.hopp-ac-popper:hover::-webkit-scrollbar-thumb,
.hopp-ac-popper.is-scrolling::-webkit-scrollbar-thumb {
  background: var(--el-border-color) !important;
}

.hopp-ac-item {
  display: block;
  box-sizing: border-box;
  width: 100%;
  height: 40px;
  border: none;
  background: transparent;
  text-align: left;
  padding: 0 16px;
  font-size: 13px;
  font-weight: 400;
  line-height: 40px;
  color: var(--el-text-color-primary);
  cursor: pointer;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.hopp-ac-item:hover,
.hopp-ac-item.is-current {
  background: rgb(0 0 0 / 4%);
}

html.dark .hopp-ac-item:hover,
html.dark .hopp-ac-item.is-current {
  background: rgb(255 255 255 / 6%);
}
</style>
