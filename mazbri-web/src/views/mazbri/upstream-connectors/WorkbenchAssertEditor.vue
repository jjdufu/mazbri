<template>
  <div class="wb-assert">
    <div class="wb-assert__head">
      <span class="wb-assert__title">{{ $t("upstreamConnector.assertTab") }}</span>
      <div class="wb-assert__tools">
        <ElPopover
          placement="bottom-end"
          :width="280"
          trigger="click"
          :append-to="appendTo || undefined"
        >
          <p class="wb-assert__help">{{ $t("upstreamConnector.assertHelp") }}</p>
          <template #reference>
            <button
              type="button"
              class="wb-assert__icon"
              :title="$t('upstreamConnector.assertHelpTitle')"
            >
              <ArtSvgIcon icon="ri:question-line" />
            </button>
          </template>
        </ElPopover>
        <button
          type="button"
          class="wb-assert__icon"
          :disabled="!model.length"
          :title="$t('upstreamConnector.clearKv')"
          @click="clear"
        >
          <ArtSvgIcon icon="ri:delete-bin-line" />
        </button>
        <button
          type="button"
          class="wb-assert__icon"
          :title="$t('upstreamConnector.addAssertion')"
          @click="add"
        >
          <ArtSvgIcon icon="ri:add-line" />
        </button>
      </div>
    </div>
    <VueDraggable
      v-if="model.length"
      class="wb-assert__table"
      :model-value="model"
      :animation="250"
      handle=".wb-assert__grip"
      draggable=".wb-assert__row"
      ghost-class="wb-assert__ghost"
      chosen-class="wb-assert__chosen"
      drag-class="wb-assert__drag"
      :custom-update="keepSortableDom"
      :on-move="onAssertMove"
      @end="onSortEnd"
    >
      <div
        v-for="(a, idx) in model"
        :key="a.uid || idx"
        class="wb-assert__row"
        :class="{ 'wb-assert__row--tail': isTail(idx) }"
      >
        <span class="wb-assert__grip-cell">
          <span
            v-if="!isTail(idx)"
            class="wb-assert__grip"
            :title="$t('upstreamConnector.kvDrag')"
          >
            <ArtSvgIcon icon="lucide:grip-vertical" />
          </span>
        </span>
        <div class="wb-assert__cell">
          <HoppAutoComplete
            :model-value="a.type"
            :options="assertTypeOptions"
            :append-to="appendTo"
            placeholder="status_range"
            @update:model-value="a.type = $event"
          />
        </div>
        <template v-if="a.type === 'status_range'">
          <div class="wb-assert__cell">
            <input
              class="wb-assert__input"
              type="number"
              min="0"
              max="599"
              :value="a.min"
              placeholder="200"
              @input="a.min = num($event)"
            />
          </div>
          <div class="wb-assert__cell">
            <input
              class="wb-assert__input"
              type="number"
              min="0"
              max="599"
              :value="a.max"
              placeholder="299"
              @input="a.max = num($event)"
            />
          </div>
        </template>
        <div v-else-if="a.type === 'status_in'" class="wb-assert__cell is-span">
          <input
            class="wb-assert__input"
            :value="(a.values || []).join(',')"
            placeholder="200,201"
            @input="setValues(idx, str($event))"
          />
        </div>
        <div v-else-if="a.type === 'latency_ms_lte'" class="wb-assert__cell is-span">
          <input
            class="wb-assert__input"
            type="number"
            min="0"
            :value="Number(a.value || 0)"
            @input="a.value = num($event)"
          />
        </div>
        <template v-else-if="a.type?.startsWith('jsonpath')">
          <div
            class="wb-assert__cell"
            :class="{ 'is-span': a.type !== 'jsonpath_equals' && a.type !== 'jsonpath_matches' }"
          >
            <input
              class="wb-assert__input"
              :value="a.path"
              placeholder="$.data.id"
              @input="a.path = str($event)"
            />
          </div>
          <div v-if="a.type === 'jsonpath_equals'" class="wb-assert__cell">
            <input
              class="wb-assert__input"
              :value="expectedText(a)"
              placeholder="expected"
              @input="setExpected(idx, str($event))"
            />
          </div>
          <div v-if="a.type === 'jsonpath_matches'" class="wb-assert__cell">
            <input
              class="wb-assert__input"
              :value="a.pattern"
              placeholder="pattern"
              @input="a.pattern = str($event)"
            />
          </div>
        </template>
        <div
          v-else-if="a.type === 'body_contains' || a.type === 'body_not_contains'"
          class="wb-assert__cell is-span"
        >
          <input
            class="wb-assert__input"
            :value="String(a.value ?? a.expected ?? '')"
            placeholder="substring"
            @input="a.value = str($event)"
          />
        </div>
        <template v-else-if="a.type === 'header_equals' || a.type === 'header_exists'">
          <div class="wb-assert__cell" :class="{ 'is-span': a.type === 'header_exists' }">
            <input
              class="wb-assert__input"
              :value="a.key || a.path"
              placeholder="Header-Name"
              @input="a.key = str($event)"
            />
          </div>
          <div v-if="a.type === 'header_equals'" class="wb-assert__cell">
            <input
              class="wb-assert__input"
              :value="String(a.value ?? a.expected ?? '')"
              placeholder="value"
              @input="a.value = str($event)"
            />
          </div>
        </template>
        <div v-else class="wb-assert__cell is-span" />
        <button
          type="button"
          class="wb-assert__del"
          :title="$t('common.actions.delete')"
          @click="remove(idx)"
        >
          <ArtSvgIcon icon="ri:delete-bin-line" />
        </button>
      </div>
    </VueDraggable>
  </div>
</template>

<script setup lang="ts">
import HoppAutoComplete from "@/components/common/hopp-autocomplete/index.vue";
import { VueDraggable } from "vue-draggable-plus";
import type { AssertionSpec } from "@/api/upstream-connectors";
import { ASSERT_TYPES } from "./workbenchHelpers";

const model = defineModel<AssertionSpec[]>({ required: true });

withDefaults(
  defineProps<{
    appendTo?: string | HTMLElement;
  }>(),
  { appendTo: undefined },
);

const assertTypeOptions = [...ASSERT_TYPES];

function newAssertUid(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `as-${Date.now().toString(36)}-${Math.random().toString(16).slice(2)}`;
}

watch(
  model,
  (list) => {
    for (const row of list) {
      if (!row.uid) row.uid = newAssertUid();
    }
  },
  { immediate: true },
);

function isTail(idx: number): boolean {
  return idx === model.value.length - 1 && !(model.value[idx]?.type || "").trim();
}

function add() {
  model.value = [...model.value, { type: "", uid: newAssertUid() }];
}

function keepSortableDom(): void {
  /* Keep Sortable DOM during drag so sibling rows can animate. */
}

function onAssertMove(evt: { related?: HTMLElement }): boolean {
  const related = evt.related;
  return !!related && !related.classList.contains("wb-assert__row--tail");
}

function onSortEnd(evt: { oldIndex?: number; newIndex?: number }): void {
  const from = evt.oldIndex;
  const to = evt.newIndex;
  if (from == null || to == null || from === to) return;
  const next = [...model.value];
  if (from < 0 || from >= next.length || to < 0 || to >= next.length) return;
  const [moved] = next.splice(from, 1);
  if (!moved) return;
  next.splice(to, 0, moved);
  model.value = next;
}

function clear() {
  model.value = [];
}

function remove(idx: number) {
  model.value = model.value.filter((_, i) => i !== idx);
}

function setValues(idx: number, raw: string) {
  const values = raw
    .split(/[,\s]+/)
    .map((s) => Number(s))
    .filter((n) => Number.isFinite(n));
  const row = model.value[idx];
  if (row) row.values = values;
}

function expectedText(a: AssertionSpec) {
  if (a.expected == null) return "";
  return typeof a.expected === "string" ? a.expected : JSON.stringify(a.expected);
}

function setExpected(idx: number, raw: string) {
  const row = model.value[idx];
  if (!row) return;
  try {
    row.expected = JSON.parse(raw);
  } catch {
    row.expected = raw;
  }
}

function str(ev: Event) {
  return (ev.target as HTMLInputElement).value;
}

function num(ev: Event) {
  const n = Number(str(ev));
  return Number.isFinite(n) ? n : 0;
}
</script>

<style scoped lang="scss">
.wb-assert {
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.wb-assert__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  min-height: 36px;
  padding: 0 2px 8px;
  flex-shrink: 0;
}
.wb-assert__title {
  font-size: 13px;
  font-weight: 400;
  color: var(--wb-text-muted, var(--el-text-color-secondary));
}
.wb-assert__tools {
  display: flex;
  align-items: center;
  gap: 2px;
}
.wb-assert__icon {
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
}
.wb-assert__help {
  margin: 0;
  font-size: 12px;
  line-height: 1.5;
  color: var(--wb-text, var(--el-text-color-primary));
}
.wb-assert__table {
  width: 100%;
  border-top: 1px solid var(--wb-divider-light, var(--el-border-color-extra-light));
}
.wb-assert__row {
  display: grid;
  grid-template-columns: 36px minmax(148px, 0.7fr) minmax(0, 1fr) minmax(0, 1fr) 40px;
  align-items: stretch;
  min-height: 38px;
  &:hover .wb-assert__grip {
    opacity: 1;
  }
}
.wb-assert__chosen {
  background: var(--wb-surface-hover, var(--el-fill-color-light));
}
.wb-assert__ghost {
  cursor: move;
  opacity: 1;
}
.wb-assert__drag {
  cursor: grabbing;
}
.wb-assert__grip-cell {
  display: flex;
  align-items: center;
  justify-content: center;
  border-right: 1px solid var(--wb-divider, var(--el-border-color));
  border-bottom: 1px solid var(--wb-divider-light, var(--el-border-color-extra-light));
}
.wb-assert__grip {
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
.wb-assert__row.wb-assert__chosen .wb-assert__grip,
.wb-assert__row.wb-assert__drag .wb-assert__grip {
  cursor: grabbing;
  opacity: 1;
}
.wb-assert__cell {
  min-width: 0;
  display: flex;
  align-items: stretch;
  border-right: 1px solid var(--wb-divider, var(--el-border-color));
  border-bottom: 1px solid var(--wb-divider-light, var(--el-border-color-extra-light));
  &.is-span {
    grid-column: span 2;
  }
  :deep(.hopp-ac-host),
  :deep(.hopp-ac__input) {
    width: 100%;
    height: 100%;
    min-height: 38px;
  }
}
.wb-assert__input {
  width: 100%;
  min-width: 0;
  border: none;
  outline: none;
  background: transparent;
  padding: 0 12px;
  font-size: 13px;
  color: var(--wb-text, var(--el-text-color-primary));
  appearance: textfield;
  &::placeholder {
    color: var(--wb-text-faint, var(--el-text-color-placeholder));
  }
  &::-webkit-inner-spin-button,
  &::-webkit-outer-spin-button {
    appearance: none;
    margin: 0;
  }
}
.wb-assert__del {
  border: none;
  border-bottom: 1px solid var(--wb-divider-light, var(--el-border-color-extra-light));
  background: transparent;
  color: var(--wb-danger, var(--el-color-danger));
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-size: 15px;
  padding: 0;
}
</style>
