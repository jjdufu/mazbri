<template>
  <CenteredDialog
    v-model="visible"
    :title="$t('upstreamConnector.commandPalette')"
    width="520px"
    dialog-class="wb-cmd-dialog"
    @opened="focusInput"
  >
    <div ref="panelEl" class="wb-cmd__panel" @keydown="onPanelKey">
      <input
        ref="inputEl"
        v-model="q"
        class="wb-cmd__input"
        :placeholder="$t('upstreamConnector.commandPalettePlaceholder')"
        :aria-label="$t('upstreamConnector.commandPalettePlaceholder')"
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
  </CenteredDialog>
</template>

<script setup lang="ts">
import CenteredDialog from "@/components/common/CenteredDialog.vue";
import { $t } from "@/locales";

export type WorkbenchCommand = {
  id: string;
  label: string;
  hint?: string;
  keywords?: string;
};

const props = defineProps<{
  commands: WorkbenchCommand[];
}>();

const visible = defineModel<boolean>({ required: true });

const emit = defineEmits<{
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

watch(visible, (open) => {
  if (!open) return;
  q.value = "";
  active.value = 0;
  void nextTick(focusInput);
});

watch(filtered, () => {
  active.value = 0;
});

function focusInput() {
  inputEl.value?.focus();
}

function close() {
  visible.value = false;
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
.wb-cmd__panel {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.wb-cmd__input {
  width: 100%;
  height: 36px;
  padding: 0 12px;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: var(--art-form-control-radius);
  background: var(--el-fill-color-blank);
  color: var(--el-text-color-primary);
  font-size: 13px;
  outline: none;
  &:focus {
    border-color: var(--el-color-primary);
  }
}
.wb-cmd__list {
  max-height: 320px;
  overflow: auto;
  display: flex;
  flex-direction: column;
  gap: 2px;
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
  color: var(--el-text-color-primary);
  font-size: 13px;
  text-align: left;
  cursor: pointer;
  kbd {
    font-size: 11px;
    color: var(--el-text-color-secondary);
  }
  &.is-active,
  &:hover {
    background: var(--el-fill-color);
  }
}
.wb-cmd__empty {
  padding: 16px 8px;
  font-size: 12px;
  color: var(--el-text-color-secondary);
  text-align: center;
}
</style>
