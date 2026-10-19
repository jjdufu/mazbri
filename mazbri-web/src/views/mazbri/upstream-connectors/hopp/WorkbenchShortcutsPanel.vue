<template>
  <div
    v-if="modelValue"
    class="wb-keys"
    role="dialog"
    aria-modal="true"
    :aria-label="$t('upstreamConnector.shortcutsTitle')"
    @keydown.esc.stop.prevent="close"
  >
    <button type="button" class="wb-keys__backdrop" tabindex="-1" @click="close" />
    <div class="wb-keys__panel">
      <div class="wb-keys__head">
        <span>{{ $t("upstreamConnector.shortcutsTitle") }}</span>
        <button type="button" class="wb-hopp-icon" @click="close">
          <ArtSvgIcon icon="ri:close-line" />
        </button>
      </div>
      <input
        ref="inputEl"
        v-model="q"
        class="wb-keys__search"
        :placeholder="$t('upstreamConnector.shortcutsSearch')"
        spellcheck="false"
      />
      <div class="wb-keys__list">
        <div v-for="row in filtered" :key="row.id" class="wb-keys__row">
          <span>{{ row.label }}</span>
          <kbd>{{ row.keys }}</kbd>
        </div>
        <div v-if="!filtered.length" class="wb-keys__empty">
          {{ $t("upstreamConnector.commandPaletteEmpty") }}
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { $t } from "@/locales";

export type ShortcutRow = { id: string; label: string; keys: string };

const props = defineProps<{
  modelValue: boolean;
  rows: ShortcutRow[];
}>();

const emit = defineEmits<{
  "update:modelValue": [boolean];
}>();

const q = ref("");
const inputEl = ref<HTMLInputElement | null>(null);

const filtered = computed(() => {
  const s = q.value.trim().toLowerCase();
  if (!s) return props.rows;
  return props.rows.filter((r) =>
    `${r.label} ${r.keys}`.toLowerCase().includes(s),
  );
});

watch(
  () => props.modelValue,
  (open) => {
    if (!open) return;
    q.value = "";
    void nextTick(() => inputEl.value?.focus());
  },
);

function close() {
  emit("update:modelValue", false);
}
</script>

<style scoped lang="scss">
.wb-keys {
  position: fixed;
  inset: 0;
  z-index: 40;
}
.wb-keys__backdrop {
  position: absolute;
  inset: 0;
  border: none;
  background: rgb(0 0 0 / 32%);
}
.wb-keys__panel {
  position: relative;
  z-index: 1;
  width: min(420px, calc(100% - 32px));
  margin: 12vh auto 0;
  background: var(--wb-surface, var(--el-bg-color));
  border: 1px solid var(--wb-divider, var(--el-border-color));
  border-radius: 8px;
  overflow: hidden;
}
.wb-keys__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px;
  font-size: 13px;
  font-weight: 600;
}
.wb-keys__search {
  width: calc(100% - 24px);
  margin: 0 12px 8px;
  height: 32px;
  padding: 0 8px;
  border: 1px solid var(--wb-divider-subtle, var(--el-border-color-extra-light));
  border-radius: 4px;
  background: var(--wb-surface-input, var(--el-fill-color-light));
  color: var(--wb-text, var(--el-text-color-primary));
  outline: none;
}
.wb-keys__list {
  max-height: 280px;
  overflow: auto;
  padding: 0 12px 12px;
}
.wb-keys__row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 8px 0;
  font-size: 13px;
  border-bottom: 1px solid var(--wb-divider-light, var(--el-border-color-extra-light));
  kbd {
    font-size: 11px;
    color: var(--wb-text-muted, var(--el-text-color-secondary));
  }
}
.wb-keys__empty {
  padding: 16px 0;
  font-size: 12px;
  color: var(--wb-text-muted, var(--el-text-color-secondary));
  text-align: center;
}
</style>
