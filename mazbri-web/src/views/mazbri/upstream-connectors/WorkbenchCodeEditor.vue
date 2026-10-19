<template>
  <div class="wb-code" :class="{ 'is-fill': fill, 'is-readonly': readonly }">
    <div v-if="showToolbar" class="wb-code__bar" :title="langLabel">
      <div class="wb-code__actions">
        <button
          v-if="!readonly && langLabel === 'JSON'"
          type="button"
          class="wb-code__icon"
          :title="$t('upstreamConnector.formatJson')"
          @click="format"
        >
          <ArtSvgIcon icon="ri:magic-line" />
        </button>
        <button
          type="button"
          class="wb-code__icon"
          :title="$t('common.actions.copy')"
          @click="copy"
        >
          <ArtSvgIcon icon="ri:file-copy-line" />
        </button>
      </div>
    </div>
    <Codemirror
      v-model="local"
      class="wb-code__cm"
      :extensions="extensions"
      :autofocus="false"
      :indent-with-tab="true"
      :tab-size="2"
      :placeholder="placeholder"
      @change="onChange"
    />
  </div>
</template>

<script setup lang="ts">
import { Codemirror } from "vue-codemirror";
import { StreamLanguage } from "@codemirror/language";
import { json as jsonMode, javascript } from "@codemirror/legacy-modes/mode/javascript";
import { xml as xmlMode } from "@codemirror/legacy-modes/mode/xml";
import { drawSelection, EditorView } from "@codemirror/view";
import { EditorState } from "@codemirror/state";
import { ElMessage } from "element-plus";
import { $t } from "@/locales";
import { prettyJson } from "./workbenchHelpers";

const props = withDefaults(
  defineProps<{
    modelValue: string;
    readonly?: boolean;
    fill?: boolean;
    height?: string;
    showToolbar?: boolean;
    placeholder?: string;
    langLabel?: "JSON" | "XML" | "GraphQL" | string;
  }>(),
  {
    readonly: false,
    fill: false,
    height: "220px",
    showToolbar: true,
    placeholder: "{}",
    langLabel: "JSON",
  },
);

const emit = defineEmits<{
  "update:modelValue": [string];
}>();

const local = ref(props.modelValue || "");

watch(
  () => props.modelValue,
  (v) => {
    if (v !== local.value) local.value = v || "";
  },
);

function languageMode() {
  if (props.langLabel === "XML") return xmlMode;
  if (props.langLabel === "JSON") return jsonMode;
  return javascript;
}

const extensions = computed(() => [
  StreamLanguage.define(languageMode()),
  drawSelection(),
  EditorView.lineWrapping,
  EditorView.theme({
    "&": {
      height: props.fill ? "100%" : props.height,
      width: "100%",
      border: "none",
      outline: "none",
      fontSize: "13px",
      backgroundColor: "transparent",
    },
    "&.cm-focused": { outline: "none" },
    ".cm-scroller": { overflow: "auto" },
    ".cm-content": {
      fontFamily: "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace",
      lineHeight: "1.6",
      padding: "8px 12px",
      caretColor: "var(--wb-text, var(--el-text-color-primary))",
      color: "var(--wb-text, var(--el-text-color-primary))",
    },
    ".cm-gutters": {
      backgroundColor: "var(--wb-surface, var(--el-bg-color))",
      color: "var(--wb-text-muted, var(--el-text-color-secondary))",
      border: "none",
      borderRight: "1px solid var(--wb-divider-light, var(--el-border-color-extra-light))",
    },
    ".cm-activeLine": {
      backgroundColor: "var(--wb-surface-input, var(--el-fill-color-light))",
    },
    ".cm-activeLineGutter": {
      backgroundColor: "var(--wb-surface-input, var(--el-fill-color-light))",
      color: "var(--wb-text, var(--el-text-color-primary))",
    },
  }),
  EditorState.readOnly.of(props.readonly),
  EditorView.editable.of(!props.readonly),
]);

function onChange(val: string) {
  local.value = val;
  emit("update:modelValue", val);
}

function format() {
  const next = prettyJson(local.value);
  local.value = next;
  emit("update:modelValue", next);
}

async function copy() {
  try {
    await navigator.clipboard.writeText(local.value || "");
    ElMessage.success($t("common.feedback.copySuccess"));
  } catch {
    ElMessage.error($t("common.feedback.copyFailed"));
  }
}
</script>

<style scoped lang="scss">
.wb-code {
  display: flex;
  flex-direction: column;
  min-height: 0;
  border: 1px solid var(--wb-divider-light, var(--el-border-color-light));
  border-radius: var(--wb-radius, var(--art-form-control-radius));
  background: var(--wb-surface, var(--el-fill-color-blank));
  overflow: hidden;
}
.wb-code.is-fill {
  flex: 1;
  height: 100%;
}
.wb-code__bar {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  padding: 2px 6px;
  background: var(--wb-surface-input, var(--el-fill-color-light));
  border-bottom: 1px solid var(--wb-divider-light, var(--el-border-color-lighter));
}
.wb-code__actions {
  display: flex;
  align-items: center;
  gap: 2px;
}
.wb-code__icon {
  width: 28px;
  height: 28px;
  border: none;
  background: transparent;
  color: var(--wb-text-muted, var(--el-text-color-secondary));
  border-radius: var(--wb-radius, var(--art-form-control-radius));
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  &:hover {
    color: var(--wb-text, var(--el-text-color-primary));
    background: var(--wb-surface-hover, var(--el-fill-color));
  }
}
.wb-code__cm {
  min-height: 0;
  flex: 1;
  background: var(--wb-surface, var(--el-bg-color));
  color: var(--wb-text, var(--el-text-color-primary));
}
.wb-code :deep(.cm-editor) {
  background: transparent;
  color: var(--wb-text, var(--el-text-color-primary));
  height: 100%;
}
.wb-code :deep(.cm-gutters) {
  background: var(--wb-surface, var(--el-bg-color));
  color: var(--wb-text-muted, var(--el-text-color-secondary));
  border: none;
  border-right: 1px solid var(--wb-divider-light, var(--el-border-color-extra-light));
}
.wb-code :deep(.cm-gutterElement) {
  color: inherit;
}
.wb-code :deep(.cm-activeLine),
.wb-code :deep(.cm-activeLineGutter) {
  background: var(--wb-surface-input, var(--el-fill-color-light));
}
.wb-code :deep(.cm-cursor) {
  border-left-color: var(--wb-text, var(--el-text-color-primary));
}
</style>
