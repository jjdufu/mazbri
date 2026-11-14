<template>
  <CenteredDialog
    v-model="visible"
    :title="$t('upstreamConnector.shortcutsTitle')"
    width="460px"
    dialog-class="wb-keys-dialog"
    @opened="focusInput"
  >
    <div class="wb-keys__panel">
      <input
        ref="inputEl"
        v-model="q"
        class="wb-keys__search"
        :placeholder="$t('upstreamConnector.shortcutsSearch')"
        :aria-label="$t('upstreamConnector.shortcutsSearch')"
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
  </CenteredDialog>
</template>

<script setup lang="ts">
import CenteredDialog from "@/components/common/CenteredDialog.vue";
import { $t } from "@/locales";

export type ShortcutRow = { id: string; label: string; keys: string };

const props = defineProps<{
  rows: ShortcutRow[];
}>();

const visible = defineModel<boolean>({ required: true });

const q = ref("");
const inputEl = ref<HTMLInputElement | null>(null);

const filtered = computed(() => {
  const s = q.value.trim().toLowerCase();
  if (!s) return props.rows;
  return props.rows.filter((r) =>
    `${r.label} ${r.keys}`.toLowerCase().includes(s),
  );
});

watch(visible, (open) => {
  if (!open) return;
  q.value = "";
  void nextTick(focusInput);
});

function focusInput() {
  inputEl.value?.focus();
}
</script>

<style scoped lang="scss">
.wb-keys__panel {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.wb-keys__search {
  width: 100%;
  height: 32px;
  padding: 0 10px;
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
.wb-keys__list {
  max-height: 280px;
  overflow: auto;
}
.wb-keys__row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 8px 0;
  font-size: 13px;
  color: var(--el-text-color-primary);
  border-bottom: 1px solid var(--el-border-color-extra-light);
  kbd {
    font-size: 11px;
    color: var(--el-text-color-secondary);
  }
}
.wb-keys__empty {
  padding: 16px 0;
  font-size: 12px;
  color: var(--el-text-color-secondary);
  text-align: center;
}
</style>
