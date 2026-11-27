<template>
  <div class="file-picker-root" :class="rootClass">
    <div
      class="file-picker"
      :class="{
        'file-picker--selected': hasSelection,
        'file-picker--dragover': isDragOver,
        'file-picker--disabled': disabled,
        [`file-picker--${size}`]: true,
      }"
      role="button"
      :tabindex="disabled ? -1 : 0"
      :aria-disabled="disabled || undefined"
      @click="open"
      @keydown.enter.prevent="open"
      @keydown.space.prevent="open"
      @dragenter.prevent="handleDragEnter"
      @dragover.prevent="handleDragOver"
      @dragleave.prevent="handleDragLeave"
      @drop.prevent="handleDrop"
    >
      <input
        ref="inputRef"
        class="file-picker__input"
        type="file"
        :accept="accept"
        :multiple="multiple"
        :disabled="disabled"
        @change="handleInputChange"
      />

      <div class="file-picker__icon">
        <slot name="icon" :selected="hasSelection" :icon="displayIcon">
          <ArtSvgIcon :icon="displayIcon" :size="resolvedIconSize" />
        </slot>
      </div>

      <div class="file-picker__content">
        <div class="file-picker__title">
          <slot
            name="title"
            :selected="hasSelection"
            :files="selectedFiles"
            :title="displayTitle"
          >
            {{ displayTitle }}
          </slot>
        </div>
        <div v-if="displayHint" class="file-picker__hint">
          <slot
            name="hint"
            :selected="hasSelection"
            :files="selectedFiles"
            :hint="displayHint"
          >
            {{ displayHint }}
          </slot>
        </div>
        <div
          v-if="displaySubtleHint"
          class="file-picker__hint file-picker__hint--subtle"
        >
          <slot
            name="subtle-hint"
            :selected="hasSelection"
            :files="selectedFiles"
            :hint="displaySubtleHint"
          >
            {{ displaySubtleHint }}
          </slot>
        </div>
      </div>

      <div class="file-picker__status">
        <slot
          name="status"
          :selected="hasSelection"
          :files="selectedFiles"
        >
          <ElTag
            v-if="showSelectedTag && hasSelection"
            type="success"
            size="small"
          >
            {{ resolvedSelectedTagText }}
          </ElTag>
          <span class="file-picker__action">
            {{ displayActionLabel }}
          </span>
        </slot>
      </div>
    </div>

    <ul
      v-if="multiple && showFileList && selectedFiles.length > 0"
      class="file-picker__list"
    >
      <li
        v-for="(file, index) in selectedFiles"
        :key="`${file.name}-${file.size}-${file.lastModified}-${index}`"
        class="file-picker__list-item"
      >
        <span class="file-picker__list-name" :title="file.name">
          {{ file.name }}
        </span>
        <span class="file-picker__list-size">
          {{ formatFileSize(file.size) }}
        </span>
        <button
          v-if="!disabled"
          type="button"
          class="file-picker__list-remove"
          :aria-label="resolvedRemoveLabel"
          @click.stop="removeAt(index)"
        >
          {{ resolvedRemoveLabel }}
        </button>
      </li>
    </ul>
  </div>
</template>

<script setup lang="ts">
import { ElMessage, ElTag } from "element-plus";
import { computed, ref } from "vue";
import { $t } from "@/locales";
import {
  formatFileSize,
  validatePickedFile,
  type FilePickerValidateResult,
} from "@/composables/useFilePicker";

export type FilePickerSize = "compact" | "default" | "large";
export type FilePickerModel = File | File[] | null;

interface Props {
  modelValue?: FilePickerModel;

  accept?: string;
  multiple?: boolean;

  maxSize?: number;
  disabled?: boolean;

  title?: string;

  hint?: string;

  subtleHint?: string;

  selectedHint?: string;
  actionLabel?: string;
  changeActionLabel?: string;
  selectedTagText?: string;
  emptyIcon?: string;
  selectedIcon?: string;
  iconSize?: number;
  size?: FilePickerSize;
  showSelectedTag?: boolean;

  showFileSize?: boolean;

  showFileList?: boolean;

  rootClass?: string;
  invalidTypeMessage?: string;
  fileTooLargeMessage?: string;
  removeLabel?: string;

  validate?: (file: File) => FilePickerValidateResult;
}

const props = withDefaults(defineProps<Props>(), {
  modelValue: null,
  accept: undefined,
  multiple: false,
  maxSize: undefined,
  disabled: false,
  title: undefined,
  hint: undefined,
  subtleHint: undefined,
  selectedHint: undefined,
  actionLabel: undefined,
  changeActionLabel: undefined,
  selectedTagText: undefined,
  emptyIcon: "ri:upload-cloud-2-line",
  selectedIcon: "ri:file-text-line",
  iconSize: undefined,
  size: "default",
  showSelectedTag: true,
  showFileSize: true,
  showFileList: true,
  rootClass: undefined,
  invalidTypeMessage: undefined,
  fileTooLargeMessage: undefined,
  removeLabel: undefined,
  validate: undefined,
});

const emit = defineEmits<{
  "update:modelValue": [value: FilePickerModel];
  change: [value: FilePickerModel];
  error: [message: string];
}>();

const inputRef = ref<HTMLInputElement | null>(null);
const isDragOver = ref(false);
const dragDepth = ref(0);

const selectedFiles = computed((): File[] => {
  const value = props.modelValue;
  if (!value) {
    return [];
  }
  return Array.isArray(value) ? value : [value];
});

const hasSelection = computed(() => selectedFiles.value.length > 0);

const singleFile = computed(() =>
  !props.multiple && selectedFiles.value.length === 1
    ? selectedFiles.value[0]
    : null,
);

const resolvedTitle = computed(
  () => props.title ?? $t("common.filePicker.defaultTitle"),
);
const resolvedHint = computed(
  () => props.hint ?? $t("common.filePicker.defaultHint"),
);
const resolvedSubtleHint = computed(() => props.subtleHint ?? "");
const resolvedSelectedHint = computed(
  () => props.selectedHint ?? $t("common.filePicker.selectedHint"),
);
const resolvedActionLabel = computed(
  () => props.actionLabel ?? $t("common.filePicker.choose"),
);
const resolvedChangeActionLabel = computed(
  () => props.changeActionLabel ?? $t("common.filePicker.change"),
);
const resolvedSelectedTagText = computed(
  () => props.selectedTagText ?? $t("common.filePicker.selected"),
);
const resolvedRemoveLabel = computed(
  () => props.removeLabel ?? $t("common.filePicker.remove"),
);
const resolvedInvalidTypeMessage = computed(
  () => props.invalidTypeMessage ?? $t("common.filePicker.invalidType"),
);

const displayIcon = computed(() =>
  hasSelection.value ? props.selectedIcon : props.emptyIcon,
);

const resolvedIconSize = computed(() => {
  if (props.iconSize !== undefined) {
    return props.iconSize;
  }
  if (props.size === "compact") {
    return 22;
  }
  if (props.size === "large") {
    return 32;
  }
  return 28;
});

const displayTitle = computed(() => {
  if (!hasSelection.value) {
    return resolvedTitle.value;
  }
  if (props.multiple) {
    return $t("common.filePicker.selectedCount", {
      count: selectedFiles.value.length,
    });
  }
  return singleFile.value?.name ?? resolvedTitle.value;
});

const displayHint = computed(() => {
  if (!hasSelection.value) {
    return resolvedHint.value;
  }
  if (props.multiple) {
    return resolvedSelectedHint.value;
  }
  return resolvedSelectedHint.value;
});

const displaySubtleHint = computed(() => {
  if (!hasSelection.value) {
    return resolvedSubtleHint.value;
  }
  if (
    !props.multiple &&
    props.showFileSize &&
    singleFile.value
  ) {
    return $t("common.filePicker.selectedSize", {
      size: formatFileSize(singleFile.value.size),
    });
  }
  return "";
});

const displayActionLabel = computed(() =>
  hasSelection.value
    ? resolvedChangeActionLabel.value
    : resolvedActionLabel.value,
);

function resetDragState() {
  isDragOver.value = false;
  dragDepth.value = 0;
}

function reportError(message: string) {
  ElMessage.error(message);
  emit("error", message);
}

function fileTooLargeMessageFor(maxSize: number) {
  if (props.fileTooLargeMessage) {
    return props.fileTooLargeMessage;
  }
  return $t("common.filePicker.fileTooLarge", {
    size: formatFileSize(maxSize),
  });
}

function checkFile(file: File): boolean {
  const result = validatePickedFile(file, {
    accept: props.accept,
    maxSize: props.maxSize,
    validate: props.validate,
    invalidTypeMessage: resolvedInvalidTypeMessage.value,
    fileTooLargeMessage: fileTooLargeMessageFor,
  });
  if (result === true) {
    return true;
  }
  reportError(result);
  return false;
}

function emitValue(value: FilePickerModel) {
  emit("update:modelValue", value);
  emit("change", value);
}

function applyFiles(incoming: File[]) {
  if (props.disabled || incoming.length === 0) {
    resetDragState();
    return;
  }

  const accepted = incoming.filter((file) => checkFile(file));
  if (accepted.length === 0) {
    resetDragState();
    return;
  }

  if (props.multiple) {
    const existing = selectedFiles.value;
    const merged: File[] = [...existing];
    for (const file of accepted) {
      const duplicate = merged.some(
        (item) =>
          item.name === file.name &&
          item.size === file.size &&
          item.lastModified === file.lastModified,
      );
      if (!duplicate) {
        merged.push(file);
      }
    }
    emitValue(merged);
  } else {
    emitValue(accepted[0] ?? null);
  }
  resetDragState();
}

function handleInputChange(event: Event) {
  const target = event.target as HTMLInputElement;
  const files = target.files ? Array.from(target.files) : [];
  applyFiles(files);

  target.value = "";
}

function open() {
  if (props.disabled) {
    return;
  }
  const input = inputRef.value;
  if (!input) {
    return;
  }
  input.value = "";
  input.click();
}

function handleDragEnter() {
  if (props.disabled) {
    return;
  }
  dragDepth.value += 1;
  isDragOver.value = true;
}

function handleDragOver() {
  if (props.disabled) {
    return;
  }
  isDragOver.value = true;
}

function handleDragLeave() {
  if (props.disabled) {
    return;
  }
  dragDepth.value = Math.max(0, dragDepth.value - 1);
  if (dragDepth.value === 0) {
    isDragOver.value = false;
  }
}

function handleDrop(event: DragEvent) {
  if (props.disabled) {
    resetDragState();
    return;
  }
  const files = event.dataTransfer?.files
    ? Array.from(event.dataTransfer.files)
    : [];
  applyFiles(files);
}

function removeAt(index: number) {
  if (!props.multiple) {
    clear();
    return;
  }
  const next = selectedFiles.value.filter((_, i) => i !== index);
  emitValue(next);
}

function clear() {
  emitValue(props.multiple ? [] : null);
  if (inputRef.value) {
    inputRef.value.value = "";
  }
  resetDragState();
}

defineExpose({
  open,
  clear,
  inputRef,
});
</script>

<style scoped lang="scss">
.file-picker-root {
  width: 100%;
}

.file-picker__input {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border: 0;
}

.file-picker {
  position: relative;
  display: flex;
  align-items: center;
  gap: 14px;
  width: 100%;
  min-height: 108px;
  padding: 18px;
  border: 2px dashed var(--file-picker-border-color, var(--el-border-color));
  border-radius: var(--file-picker-radius, var(--art-card-radius));
  background: linear-gradient(
    180deg,
    var(--el-fill-color-blank),
    var(--el-fill-color-lighter)
  );
  cursor: pointer;
  transition:
    border-color 0.2s ease,
    background-color 0.2s ease,
    box-shadow 0.2s ease,
    transform 0.2s ease;

  &:hover:not(.file-picker--disabled) {
    border-color: var(--el-color-primary);
    box-shadow: 0 12px 24px rgba(15, 23, 42, 0.06);
    transform: translateY(-1px);
  }

  &:focus-visible {
    outline: 2px solid var(--el-color-primary);
    outline-offset: 2px;
  }

  &--compact {
    min-height: 84px;
    padding: 12px 14px;
    gap: 12px;
  }

  &--large {
    min-height: 128px;
    padding: 22px;
  }

  &--selected {
    border-style: solid;
    border-color: var(
      --file-picker-selected-border,
      var(--el-color-success)
    );
    background: linear-gradient(
      180deg,
      rgba(103, 194, 58, 0.08),
      rgba(103, 194, 58, 0.03)
    );
  }

  &--dragover {
    border-color: var(--el-color-primary);
    background: linear-gradient(
      180deg,
      rgba(64, 158, 255, 0.16),
      rgba(64, 158, 255, 0.06)
    );
    box-shadow: 0 14px 28px rgba(64, 158, 255, 0.12);
  }

  &--disabled {
    cursor: not-allowed;
    opacity: 0.6;
    transform: none;
    box-shadow: none;
  }
}

.file-picker__icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 44px;
  height: 44px;
  border-radius: var(--file-picker-radius, var(--art-card-radius));
  background: rgba(64, 158, 255, 0.12);
  color: var(--el-color-primary);
  flex-shrink: 0;
}

.file-picker--compact .file-picker__icon {
  width: 36px;
  height: 36px;
}

.file-picker--selected .file-picker__icon {
  background: rgba(103, 194, 58, 0.15);
  color: var(--el-color-success);
}

.file-picker__content {
  flex: 1;
  min-width: 0;
}

.file-picker__title {
  font-weight: 600;
  color: var(--el-text-color-primary);
  word-break: break-all;
}

.file-picker__hint {
  margin-top: 4px;
  color: var(--el-text-color-secondary);
  font-size: 12px;
  line-height: 1.5;
}

.file-picker__hint--subtle {
  color: var(--el-text-color-placeholder);
}

.file-picker__status {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 6px;
  flex-shrink: 0;
}

.file-picker__action {
  color: var(--el-color-primary);
  font-size: 12px;
  font-weight: 600;
}

.file-picker--selected .file-picker__action {
  color: var(--el-color-success);
}

.file-picker__list {
  margin: 12px 0 0;
  padding: 0;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.file-picker__list-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  border-radius: var(--file-picker-radius, var(--art-card-radius));
  background: var(--el-fill-color-lighter);
  border: 1px solid var(--el-border-color-lighter);
  font-size: 13px;
}

.file-picker__list-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--el-text-color-primary);
}

.file-picker__list-size {
  flex-shrink: 0;
  color: var(--el-text-color-secondary);
  font-size: 12px;
}

.file-picker__list-remove {
  flex-shrink: 0;
  border: none;
  background: transparent;
  color: var(--el-color-danger);
  cursor: pointer;
  font-size: 12px;
  padding: 0;
  line-height: 1.2;

  &:hover {
    text-decoration: underline;
  }
}
</style>
