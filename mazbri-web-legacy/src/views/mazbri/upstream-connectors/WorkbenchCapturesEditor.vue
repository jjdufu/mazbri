<template>
  <div class="wb-cap">
    <div class="wb-cap__head">
      <span class="wb-cap__title">{{ $t("upstreamConnector.capturesTab") }}</span>
      <div class="wb-cap__tools">
        <button
          type="button"
          class="wb-cap__icon"
          :disabled="!model.length || disabled"
          :title="$t('upstreamConnector.clearKv')"
          @click="clearRows"
        >
          <ArtSvgIcon icon="ri:delete-bin-line" />
        </button>
        <button
          type="button"
          class="wb-cap__icon"
          :disabled="disabled"
          :title="$t('upstreamConnector.addCapture')"
          @click="addRow"
        >
          <ArtSvgIcon icon="ri:add-line" />
        </button>
      </div>
    </div>

    <div v-if="model.length" class="wb-cap__table">
      <div
        v-for="(row, idx) in model"
        :key="row.uid || idx"
        class="wb-cap__row"
      >
        <input
          class="wb-cap__input is-mono"
          :value="row.name"
          :disabled="disabled"
          :placeholder="$t('upstreamConnector.captureNamePh')"
          :aria-label="$t('upstreamConnector.fields.envName')"
          spellcheck="false"
          @input="patchRow(idx, { name: ($event.target as HTMLInputElement).value })"
        />
        <input
          class="wb-cap__input is-mono"
          :value="row.path"
          :disabled="disabled"
          :placeholder="$t('upstreamConnector.capturePathPh')"
          aria-label="JSONPath"
          spellcheck="false"
          @input="patchRow(idx, { path: ($event.target as HTMLInputElement).value })"
        />
        <button
          type="button"
          class="wb-cap__copy"
          :disabled="!row.name.trim()"
          :title="$t('common.actions.copy')"
          @click="copyPlaceholder(row.name)"
        >
          <code>{{ row.name.trim() ? `\{\{captures.${row.name.trim()}\}\}` : "\{\{captures.*\}\}" }}</code>
        </button>
        <button
          type="button"
          class="wb-cap__del"
          :disabled="disabled"
          :title="$t('common.actions.delete')"
          @click="removeRow(idx)"
        >
          <ArtSvgIcon icon="ri:delete-bin-line" />
        </button>
      </div>
    </div>

    <div class="wb-cap__pool">
      <div class="wb-cap__pool-head">
        <span class="wb-cap__title">
          {{ $t("upstreamConnector.capturedPoolTitle") }}
          <span class="wb-cap__badge tabular-nums">{{ capturedEntries.length }}</span>
        </span>
        <button
          v-if="capturedEntries.length"
          type="button"
          class="wb-hopp-action"
          :disabled="disabled"
          @click="emit('clear-captured')"
        >
          {{ $t("upstreamConnector.clearKv") }}
        </button>
      </div>

      <div v-if="!capturedEntries.length" class="wb-cap__empty">
        {{ $t("upstreamConnector.capturedEmpty") }}
      </div>
      <div v-else class="wb-cap__pool-table">
        <div
          v-for="[k, v] in capturedEntries"
          :key="k"
          class="wb-cap__pool-row"
        >
          <button
            type="button"
            class="wb-cap__token"
            :title="$t('common.actions.copy')"
            @click="copyPlaceholder(k)"
          >
            <code>{{ `\{\{captures.${k}\}\}` }}</code>
            <ArtSvgIcon icon="ri:file-copy-line" />
          </button>
          <span class="wb-cap__val">{{ v }}</span>
          <button
            v-if="!disabled"
            type="button"
            class="wb-cap__del"
            :title="$t('common.actions.delete')"
            @click="emit('remove-captured', k)"
          >
            <ArtSvgIcon icon="ri:close-line" />
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { CaptureSpec } from "@/api/upstream-connectors";
import { copyWithFeedback } from "@/utils/ui/copy-feedback";
import { normalizeCaptureRow } from "./workbenchHelpers";

const model = defineModel<CaptureSpec[]>({ required: true });

const props = withDefaults(
  defineProps<{
    capturedVars: Record<string, string>;
    disabled?: boolean;
  }>(),
  { disabled: false },
);

const emit = defineEmits<{
  "clear-captured": [];
  "remove-captured": [string];
}>();

const capturedEntries = computed(() => Object.entries(props.capturedVars || {}));

function addRow() {
  model.value = [...model.value, normalizeCaptureRow({ name: "", path: "" })];
}

function patchRow(idx: number, patch: Partial<CaptureSpec>) {
  const next = model.value.map((r, i) =>
    i === idx ? normalizeCaptureRow({ ...r, ...patch }) : normalizeCaptureRow(r),
  );
  const last = next[next.length - 1];
  if (last && (last.name.trim() || last.path.trim())) {
    next.push(normalizeCaptureRow({ name: "", path: "" }));
  }
  model.value = next;
}

function removeRow(idx: number) {
  model.value = model.value.filter((_, i) => i !== idx);
}

function clearRows() {
  model.value = [];
}

async function copyPlaceholder(name: string) {
  const trimmed = name.trim();
  if (!trimmed) return;
  await copyWithFeedback(`{{captures.${trimmed}}}`);
}
</script>

<style scoped lang="scss">
.wb-cap {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.wb-cap__head,
.wb-cap__pool-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  min-height: 32px;
  padding: 0 2px;
}

.wb-cap__title {
  display: inline-flex;
  gap: 6px;
  align-items: center;
  font-size: 12.5px;
  font-weight: 600;
  color: var(--wb-text-muted);
}

.wb-cap__badge {
  padding: 0 6px;
  font-family: var(--wb-font-mono);
  font-size: 11px;
  color: var(--wb-text-muted);
  background: var(--wb-surface-subtle);
  border: 1px solid var(--wb-divider);
  border-radius: 999px;
}

.wb-cap__tools {
  display: flex;
  gap: 2px;
  align-items: center;
}

.wb-cap__icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  color: var(--wb-text-muted);
  cursor: pointer;
  background: transparent;
  border: none;
  border-radius: var(--wb-radius);

  &:hover:not(:disabled) {
    color: var(--wb-text);
    background: var(--wb-surface-hover);
  }

  &:disabled {
    cursor: default;
    opacity: 0.4;
  }
}

.wb-cap__table,
.wb-cap__pool-table {
  border-top: 1px solid var(--wb-divider);
  border-left: 1px solid var(--wb-divider);
}

.wb-cap__row {
  display: grid;
  grid-template-columns: minmax(140px, 0.8fr) minmax(180px, 1.2fr) minmax(150px, 0.9fr) 36px;
  min-height: 36px;

  > * {
    border-right: 1px solid var(--wb-divider);
    border-bottom: 1px solid var(--wb-divider);
  }
}

.wb-cap__input {
  width: 100%;
  min-width: 0;
  padding: 0 10px;
  font-size: 12.5px;
  color: var(--wb-text);
  background: transparent;
  border: none;
  outline: none;

  &.is-mono {
    font-family: var(--wb-font-mono);
    font-size: 12px;
  }

  &::placeholder {
    color: var(--wb-text-faint);
  }
}

.wb-cap__copy {
  display: inline-flex;
  align-items: center;
  justify-content: flex-start;
  padding: 0 10px;
  overflow: hidden;
  color: var(--wb-text-muted);
  cursor: pointer;
  background: var(--wb-surface-subtle);
  border: none;

  code {
    overflow: hidden;
    text-overflow: ellipsis;
    font-family: var(--wb-font-mono);
    font-size: 11px;
    white-space: nowrap;
  }

  &:hover:not(:disabled) {
    color: var(--wb-text);
    background: var(--wb-surface-hover);
  }

  &:disabled {
    cursor: default;
    opacity: 0.45;
  }
}

.wb-cap__del {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--wb-danger);
  cursor: pointer;
  background: transparent;
  border: none;

  &:hover:not(:disabled) {
    background: var(--wb-surface-hover);
  }
}

.wb-cap__pool {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.wb-cap__empty {
  padding: 14px 12px;
  font-size: 12px;
  color: var(--wb-text-faint);
  background: var(--wb-surface-subtle);
  border: 1px solid var(--wb-divider);
  border-radius: var(--wb-radius);
}

.wb-cap__pool-row {
  display: grid;
  grid-template-columns: minmax(160px, 0.9fr) minmax(0, 1.5fr) 36px;
  align-items: stretch;
  min-height: 34px;

  > * {
    border-right: 1px solid var(--wb-divider);
    border-bottom: 1px solid var(--wb-divider);
  }
}

.wb-cap__token {
  display: inline-flex;
  gap: 6px;
  align-items: center;
  justify-content: space-between;
  padding: 0 10px;
  font-size: 11.5px;
  color: var(--wb-success);
  cursor: pointer;
  background: var(--wb-surface-subtle);
  border: none;

  code {
    overflow: hidden;
    text-overflow: ellipsis;
    font-family: var(--wb-font-mono);
    white-space: nowrap;
  }

  &:hover {
    background: var(--wb-surface-hover);
  }
}

.wb-cap__val {
  display: flex;
  align-items: center;
  padding: 0 10px;
  overflow: hidden;
  text-overflow: ellipsis;
  font-family: var(--wb-font-mono);
  font-size: 12px;
  color: var(--wb-text);
  white-space: nowrap;
}
</style>
