<template>
  <div
    class="wb-kv"
    :class="{
      'is-keys': isKeys,
      'is-folded': folded,
      'is-section': emptyCompact || foldable,
    }"
  >
    <div
      class="wb-kv__head"
      :class="{ 'is-toggle': foldable }"
      @click="onHeadClick"
    >
      <span class="wb-kv__title">{{ title }}</span>
      <span v-if="foldable && folded" class="wb-kv__summary">{{ foldSummary }}</span>
      <div class="wb-kv__tools" @click.stop>
        <ElPopover
          placement="bottom-end"
          :width="280"
          trigger="click"
          :append-to="appendTo || undefined"
        >
          <p class="wb-kv__help">{{ helpText }}</p>
          <template #reference>
            <button
              type="button"
              class="wb-kv__icon"
              :title="helpTitle || $t('upstreamConnector.kvHelp')"
            >
              <ArtSvgIcon icon="ri:question-line" />
            </button>
          </template>
        </ElPopover>
        <button
          type="button"
          class="wb-kv__icon"
          :disabled="!canClear"
          :title="clearTitle || $t('upstreamConnector.clearKv')"
          @click="clear"
        >
          <ArtSvgIcon icon="ri:delete-bin-line" />
        </button>
        <button
          type="button"
          class="wb-kv__icon"
          :class="{ 'is-on': bulkOpen }"
          :title="$t('upstreamConnector.kvBulk')"
          @click="toggleBulk"
        >
          <ArtSvgIcon icon="ri:edit-line" />
        </button>
        <button
          type="button"
          class="wb-kv__icon"
          :title="addTitle || title"
          @click="add"
        >
          <ArtSvgIcon icon="ri:add-line" />
        </button>
      </div>
    </div>
    <textarea
      v-if="!folded && bulkOpen"
      v-model="bulkText"
      class="wb-kv__bulk"
      :placeholder="$t('upstreamConnector.kvBulkHint')"
      @blur="applyBulk"
    />
    <VueDraggable
      v-else-if="!folded && tableVisible"
      :model-value="rows"
      class="wb-kv__table"
      :animation="250"
      handle=".wb-kv__grip"
      draggable=".wb-kv__row"
      ghost-class="wb-kv__ghost"
      chosen-class="wb-kv__chosen"
      drag-class="wb-kv__drag"
      :custom-update="keepSortableDom"
      :on-move="onKvMove"
      @end="onSortEnd"
    >
      <div
        v-for="(row, idx) in rows"
        :key="row.uid || idx"
        class="wb-kv__row"
        data-draggable="true"
        :class="{
          'is-off': row.enabled === false,
          'wb-kv__row--tail': idx === rows.length - 1,
        }"
      >
        <span class="wb-kv__grip-cell">
          <span
            v-if="idx !== rows.length - 1"
            class="wb-kv__grip"
            :title="$t('upstreamConnector.kvDrag')"
          >
            <ArtSvgIcon icon="lucide:grip-vertical" />
          </span>
        </span>
        <HoppAutoComplete
          v-if="isHeaderKey"
          :model-value="row.key"
          :options="COMMON_HEADERS"
          :placeholder="keyPh"
          :append-to="appendTo"
          @update:model-value="patchRow(idx, { key: $event })"
        />
        <input
          v-else
          class="wb-kv__cell"
          :value="row.key"
          :placeholder="keyPh"
          spellcheck="false"
          autocomplete="off"
          @input="patchRow(idx, { key: ($event.target as HTMLInputElement).value })"
        />
        <template v-if="!isKeys">
          <WorkbenchEnvText
            :model-value="row.value"
            :env="env"
            :placeholder="valuePh"
            :secret="secretValues"
            @update:model-value="patchRow(idx, { value: $event })"
          />
          <input
            class="wb-kv__cell"
            :value="row.desc"
            :placeholder="descPh"
            spellcheck="false"
            autocomplete="off"
            @input="patchRow(idx, { desc: ($event.target as HTMLInputElement).value })"
          />
        </template>
        <button
          type="button"
          class="wb-kv__enable"
          :class="{ 'is-off': row.enabled === false }"
          :title="
            row.enabled === false
              ? $t('upstreamConnector.kvEnable')
              : $t('upstreamConnector.kvDisable')
          "
          @click="toggleEnabled(idx)"
        >
          <ArtSvgIcon icon="ri:check-fill" />
        </button>
        <button
          type="button"
          class="wb-kv__del"
          :title="$t('common.actions.delete')"
          @click="remove(idx)"
        >
          <ArtSvgIcon icon="ri:delete-bin-line" />
        </button>
      </div>
    </VueDraggable>
    <p v-if="!folded && dupKeys.length" class="wb-kv__dup">
      {{ $t("upstreamConnector.kvDupWarn", { keys: dupKeys.join(", ") }) }}
    </p>
  </div>
</template>

<script setup lang="ts">
import { $t } from "@/locales";
import HoppAutoComplete from "@/components/common/hopp-autocomplete/index.vue";
import { VueDraggable } from "vue-draggable-plus";
import type { KvRow } from "./workbenchHelpers";
import {
  duplicateKvKeys,
  emptyKvRow,
  isKvBlank,
  normalizeKvRow,
  parseKvBulk,
  serializeKvBulk,
} from "./workbenchHelpers";
import WorkbenchEnvText from "./hopp/WorkbenchEnvText.vue";
import { COMMON_HEADERS } from "./hopp/commonHeaders";

/**
 * Hoppscotch Header List rules (no column-header row; empty table is legal):
 * 1. v-for on modelValue — no displayRows / trailing ghost / .wb-kv__cols.
 * 2. New table may be [] or one blank row. Placeholders Key / Value / Description are on the inputs.
 * 3. Typing a non-empty key or value (or desc) into the last row immediately push()es one blank row.
 * 4. Clearing the last row while the previous is also blank collapses to a single blank (do not stack).
 * 5. + always push()es a blank row. A trailing blank plus + allows two blanks.
 * 6. Data rows have grip, Key, Value, Description, ✓, delete. Last row is the empty entry: no grip (Hoppscotch).
 * 7. Deleting the only row (including a blank) → modelValue === []. Toolbar stays; use + to add.
 * 8. Clear → []. Disabled when there are no rows.
 * 9. ✓ toggles enabled on every row, including blanks. Send ignores rows without a key.
 * 10. Help ? is clickable (which rows are sent, desc is not, unchecked kept, + after empty).
 * emptyCompact: hide the table body when there is no real data (auth / transform lists).
 * foldable: collapse the body; + still adds a row and unfolds.
 */

const props = withDefaults(
  defineProps<{
    modelValue: Array<Partial<KvRow> & { key?: string; value?: string }>;
    title: string;
    addTitle?: string;
    clearTitle?: string;
    helpTitle?: string;
    keyPlaceholder?: string;
    valuePlaceholder?: string;
    descPlaceholder?: string;
    env?: Record<string, string>;
    variant?: "default" | "headers" | "keys";
    secretValues?: boolean;
    emptyCompact?: boolean;
    foldable?: boolean;
    appendTo?: string | HTMLElement;
  }>(),
  {
    addTitle: "",
    clearTitle: "",
    helpTitle: "",
    keyPlaceholder: "",
    valuePlaceholder: "",
    descPlaceholder: "",
    env: () => ({}),
    variant: "default",
    secretValues: false,
    emptyCompact: false,
    foldable: false,
    appendTo: undefined,
  },
);

const emit = defineEmits<{
  "update:modelValue": [KvRow[]];
}>();

const bulkOpen = ref(false);
const bulkText = ref("");
const forceShow = ref(false);
const userFolded = ref<boolean | null>(null);

const isKeys = computed(() => props.variant === "keys");
const isHeaderKey = computed(
  () => props.variant === "headers" || props.variant === "keys",
);

const keyPh = computed(
  () => props.keyPlaceholder || $t("upstreamConnector.kvPhKey"),
);
const valuePh = computed(
  () => props.valuePlaceholder || $t("upstreamConnector.kvPhValue"),
);
const descPh = computed(
  () => props.descPlaceholder || $t("upstreamConnector.kvPhDesc"),
);

function asRows(
  value: Array<Partial<KvRow> & { key?: string; value?: string }> | undefined,
): KvRow[] {
  return Array.isArray(value) ? value.map(normalizeKvRow) : [];
}

const rows = computed(() => asRows(props.modelValue));

const filledCount = computed(
  () => rows.value.filter((row) => !isKvBlank(row)).length,
);

const hasData = computed(() => filledCount.value > 0);

const folded = computed(() => {
  if (!props.foldable) return false;
  if (userFolded.value != null) return userFolded.value;
  return !hasData.value;
});

const foldSummary = computed(() => {
  if (hasData.value) {
    return $t("upstreamConnector.sectionItemCount", { n: filledCount.value });
  }
  return $t("upstreamConnector.sectionUnconfigured");
});

const canClear = computed(() => rows.value.length > 0);

const dupKeys = computed(() => duplicateKvKeys(rows.value));

const helpText = computed(
  () => props.helpTitle || $t("upstreamConnector.kvHelpDetail"),
);

const tableVisible = computed(() => {
  if (rows.value.length === 0) return false;
  if (!props.emptyCompact) return true;
  if (forceShow.value) return true;
  return hasData.value;
});

function emitRows(next: KvRow[]) {
  emit("update:modelValue", next.map(normalizeKvRow));
}

function afterEdit(next: KvRow[]) {
  let edited = next.map(normalizeKvRow);
  if (edited.length === 0) {
    emitRows([]);
    return;
  }
  const last = edited[edited.length - 1]!;
  if (!isKvBlank(last)) {
    edited = [...edited, emptyKvRow()];
  } else {
    while (
      edited.length > 1 &&
      isKvBlank(edited[edited.length - 1]!) &&
      isKvBlank(edited[edited.length - 2]!)
    ) {
      edited = edited.slice(0, -1);
    }
  }
  emitRows(edited);
}

function patchRow(idx: number, patch: Partial<KvRow>) {
  const base = asRows(props.modelValue);
  if (idx < 0 || idx >= base.length) return;
  base[idx] = normalizeKvRow({ ...base[idx], ...patch });
  afterEdit(base);
}

function add() {
  bulkOpen.value = false;
  forceShow.value = true;
  if (props.foldable) userFolded.value = false;
  const base = asRows(props.modelValue);
  if (props.emptyCompact && (base.length === 0 || base.every(isKvBlank))) {
    emitRows([emptyKvRow()]);
    return;
  }
  emitRows([...base, emptyKvRow()]);
}

function remove(idx: number) {
  emitRows(asRows(props.modelValue).filter((_, i) => i !== idx));
}

function clear() {
  if (!canClear.value) return;
  forceShow.value = false;
  emitRows([]);
}

function toggleEnabled(idx: number) {
  emitRows(
    asRows(props.modelValue).map((row, i) =>
      i === idx ? normalizeKvRow({ ...row, enabled: row.enabled === false }) : row,
    ),
  );
}

function onHeadClick() {
  if (!props.foldable) return;
  userFolded.value = !folded.value;
}

function toggleBulk() {
  if (props.foldable) userFolded.value = false;
  if (bulkOpen.value) {
    applyBulk();
    bulkOpen.value = false;
    return;
  }
  bulkText.value = serializeKvBulk(asRows(props.modelValue));
  bulkOpen.value = true;
}

function applyBulk() {
  emitRows(parseKvBulk(bulkText.value));
}

function keepSortableDom(): void {
  /* vue-draggable-plus default onUpdate reinserts the node and splices v-model, which clears Sortable's transform animation. */
}

function onKvMove(evt: { related?: HTMLElement }): boolean {
  const related = evt.related;
  return !!related && !related.classList.contains("wb-kv__row--tail");
}

function onSortEnd(evt: { oldIndex?: number; newIndex?: number }): void {
  const from = evt.oldIndex;
  const to = evt.newIndex;
  if (from == null || to == null || from === to) return;
  const next = asRows(props.modelValue);
  if (from < 0 || from >= next.length || to < 0 || to >= next.length) return;
  const [moved] = next.splice(from, 1);
  if (!moved) return;
  next.splice(to, 0, moved);
  emitRows(next);
}
</script>

<style scoped lang="scss">
.wb-kv {
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.wb-kv__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  min-height: 36px;
  padding: 0 2px 8px;
  flex-shrink: 0;
  &.is-toggle {
    cursor: pointer;
  }
}
.wb-kv__title {
  font-size: 13px;
  font-weight: 400;
  color: var(--wb-text-muted, var(--el-text-color-secondary));
}
.wb-kv__summary {
  margin-left: auto;
  font-size: 12px;
  font-weight: 400;
  color: var(--wb-text-faint, var(--el-text-color-placeholder));
  white-space: nowrap;
}
.wb-kv__tools {
  display: flex;
  align-items: center;
  gap: 2px;
}
.wb-kv.is-folded .wb-kv__head {
  padding-bottom: 0;
}
.wb-kv.is-section .wb-kv__summary {
  margin-left: 8px;
}
.wb-kv.is-section .wb-kv__tools {
  margin-left: auto;
}
.wb-kv__icon {
  width: 36px;
  height: 36px;
  border: none;
  background: transparent;
  color: var(--wb-text-muted, var(--el-text-color-secondary));
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--wb-radius, var(--art-form-control-radius));
  font-size: 16px;
  padding: 0;
  &:hover:not(:disabled) {
    background: var(--wb-surface-hover, var(--el-fill-color));
    color: var(--wb-text, var(--el-text-color-primary));
  }
  &:disabled {
    opacity: 0.35;
    cursor: default;
  }
  &.is-on {
    color: var(--wb-accent, var(--el-color-primary));
  }
}
.wb-kv__help {
  margin: 0;
  font-size: 12px;
  line-height: 1.5;
  color: var(--wb-text, var(--el-text-color-primary));
}
.wb-kv__bulk {
  width: 100%;
  min-height: 160px;
  border: 1px solid var(--wb-divider-light, var(--el-border-color-extra-light));
  border-radius: var(--wb-radius, var(--art-form-control-radius));
  padding: 10px 12px;
  font-family: var(
    --wb-font-mono,
    ui-monospace,
    SFMono-Regular,
    Menlo,
    Monaco,
    Consolas,
    monospace
  );
  font-size: 13px;
  line-height: 1.5;
  resize: vertical;
  outline: none;
  background: var(--wb-surface, var(--el-bg-color));
  color: var(--wb-text, var(--el-text-color-primary));
}
.wb-kv__table {
  width: 100%;
  border-top: 1px solid var(--wb-divider-light, var(--el-border-color-extra-light));
}
.wb-kv__row {
  display: grid;
  grid-template-columns: 36px minmax(0, 1fr) minmax(0, 1fr) minmax(0, 1fr) 40px 40px;
  align-items: stretch;
  min-height: 38px;
  :deep(.hopp-ac-host),
  :deep(.wb-env-wrap) {
    min-width: 0;
    height: 100%;
  }
  > * {
    border-right: 1px solid var(--wb-divider, var(--el-border-color));
    border-bottom: 1px solid var(--wb-divider-light, var(--el-border-color-extra-light));
  }
  > *:last-child {
    border-right: none;
  }
  &.is-off .wb-kv__cell,
  &.is-off :deep(.wb-env),
  &.is-off :deep(.hopp-ac-host) {
    opacity: 0.5;
  }
  &:hover .wb-kv__grip {
    opacity: 1;
  }
}
.wb-kv__chosen {
  background: var(--wb-surface-hover, var(--el-fill-color-light));
}
.wb-kv__ghost {
  cursor: move;
  opacity: 1;
}
.wb-kv__drag {
  cursor: grabbing;
}
.wb-kv.is-keys .wb-kv__row {
  grid-template-columns: 36px minmax(0, 1fr) 40px 40px;
}
.wb-kv__grip-cell {
  display: flex;
  align-items: center;
  justify-content: center;
}
.wb-kv__grip {
  width: 100%;
  height: 100%;
  border: none;
  background: transparent;
  color: var(--wb-text-muted, var(--el-text-color-secondary));
  opacity: 0;
  cursor: grab;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  font-size: 14px;
  user-select: none;
  &:active {
    cursor: grabbing;
  }
}
.wb-kv__row.wb-kv__chosen .wb-kv__grip,
.wb-kv__row.wb-kv__drag .wb-kv__grip {
  cursor: grabbing;
  opacity: 1;
}
.wb-kv__cell {
  width: 100%;
  min-width: 0;
  border: 0;
  outline: none;
  background: transparent;
  box-shadow: none;
  padding: 0 12px;
  font-size: 13px;
  color: var(--wb-text, var(--el-text-color-primary));
  &::placeholder {
    color: var(--wb-text-faint, var(--el-text-color-placeholder));
  }
}
.wb-kv__row > .wb-kv__cell {
  border-right: 1px solid var(--wb-divider, var(--el-border-color));
  border-bottom: 1px solid var(--wb-divider-light, var(--el-border-color-extra-light));
}
.wb-kv__enable,
.wb-kv__del {
  border: 0;
  background: transparent;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-size: 15px;
  padding: 0;
}
.wb-kv__row > .wb-kv__enable,
.wb-kv__row > .wb-kv__del {
  border-right: 1px solid var(--wb-divider, var(--el-border-color));
  border-bottom: 1px solid var(--wb-divider-light, var(--el-border-color-extra-light));
}
.wb-kv__row > .wb-kv__del {
  border-right: none;
}
.wb-kv__enable {
  color: var(--wb-success, var(--el-color-success));
  &.is-off {
    color: var(--wb-text-faint, var(--el-text-color-placeholder));
  }
}
.wb-kv__del {
  color: var(--wb-danger, var(--el-color-danger));
}
.wb-kv__dup {
  margin: 6px 4px 0;
  font-size: 12px;
  color: var(--wb-warning, var(--el-color-warning));
}
</style>
