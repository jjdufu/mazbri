<template>
  <div
    v-if="modelValue"
    class="wb-cmd"
    role="dialog"
    aria-modal="true"
    :aria-label="$t('upstreamConnector.commandPalette')"
    @keydown.esc.stop.prevent="close"
  >
    <button type="button" class="wb-cmd__backdrop" tabindex="-1" @click="close" />
    <div ref="panelEl" class="wb-cmd__panel" @keydown="onPanelKey">
      <input
        ref="inputEl"
        v-model="q"
        class="wb-cmd__input"
        :placeholder="$t('upstreamConnector.commandPalettePlaceholder')"
        spellcheck="false"
        @keydown.enter.prevent="runActive"
      />
      <div class="wb-cmd__list" role="listbox">
        <button
          v-for="(cmd, i) in filtered"
          :key="cmd.id"
          type="button"
          class="wb-cmd__item"
          :class="{ 'is-active': i === active }"
          role="option"
          :aria-selected="i === active"
          @mouseenter="active = i"
          @click="run(cmd)"
        >
          <span>{{ cmd.label }}</span>
          <kbd v-if="cmd.hint">{{ cmd.hint }}</kbd>
        </button>
        <div v-if="!filtered.length" class="wb-cmd__empty">
          {{ $t("upstreamConnector.commandPaletteEmpty") }}
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { $t } from "@/locales";

export type WorkbenchCommand = {
  id: string;
  label: string;
  hint?: string;
  keywords?: string;
};

const props = defineProps<{
  modelValue: boolean;
  commands: WorkbenchCommand[];
}>();

const emit = defineEmits<{
  "update:modelValue": [boolean];
  run: [string];
}>();

const q = ref("");
const active = ref(0);
const inputEl = ref<HTMLInputElement | null>(null);
const panelEl = ref<HTMLElement | null>(null);

const filtered = computed(() => {
  const s = q.value.trim().toLowerCase();
  if (!s) return props.commands;
  return props.commands.filter((c) => {
    const blob = `${c.label} ${c.keywords || ""} ${c.id}`.toLowerCase();
    return blob.includes(s);
  });
});

watch(
  () => props.modelValue,
  (open) => {
    if (!open) return;
    q.value = "";
    active.value = 0;
    void nextTick(() => inputEl.value?.focus());
  },
);

watch(filtered, () => {
  active.value = 0;
});

function close() {
  emit("update:modelValue", false);
}

function run(cmd: WorkbenchCommand) {
  emit("run", cmd.id);
  close();
}

function runActive() {
  const cmd = filtered.value[active.value];
  if (cmd) run(cmd);
}

function onPanelKey(e: KeyboardEvent) {
  if (e.key === "ArrowDown") {
    e.preventDefault();
    if (!filtered.value.length) return;
    active.value = (active.value + 1) % filtered.value.length;
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    if (!filtered.value.length) return;
    active.value =
      (active.value - 1 + filtered.value.length) % filtered.value.length;
  } else if (e.key === "Tab") {
    e.preventDefault();
    inputEl.value?.focus();
  }
}
</script>

<style scoped lang="scss">
.wb-cmd {
  position: fixed;
  inset: 0;
  z-index: 40;
}
.wb-cmd__backdrop {
  position: absolute;
  inset: 0;
  border: none;
  background: rgb(0 0 0 / 32%);
}
.wb-cmd__panel {
  position: relative;
  z-index: 1;
  width: min(520px, calc(100% - 32px));
  margin: 12vh auto 0;
  background: var(--wb-surface, var(--el-bg-color));
  border: 1px solid var(--wb-divider, var(--el-border-color));
  border-radius: 8px;
  box-shadow: var(--el-box-shadow-light);
  overflow: hidden;
}
.wb-cmd__input {
  width: 100%;
  height: 44px;
  padding: 0 14px;
  border: none;
  border-bottom: 1px solid var(--wb-divider-subtle, var(--el-border-color-extra-light));
  background: transparent;
  color: var(--wb-text, var(--el-text-color-primary));
  font-size: 14px;
  outline: none;
}
.wb-cmd__list {
  max-height: 320px;
  overflow: auto;
  padding: 6px;
}
.wb-cmd__item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  gap: 12px;
  padding: 8px 10px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--wb-text, var(--el-text-color-primary));
  font-size: 13px;
  text-align: left;
  cursor: pointer;
  kbd {
    font-size: 11px;
    color: var(--wb-text-muted, var(--el-text-color-secondary));
  }
  &.is-active,
  &:hover {
    background: var(--wb-surface-hover, var(--el-fill-color));
  }
}
.wb-cmd__empty {
  padding: 16px 8px;
  font-size: 12px;
  color: var(--wb-text-muted, var(--el-text-color-secondary));
  text-align: center;
}
</style>
