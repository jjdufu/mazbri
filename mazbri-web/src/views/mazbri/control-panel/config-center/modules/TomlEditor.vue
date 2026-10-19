<template>
  <div
    class="toml-editor"
    :class="{
      'is-readonly': readonly,
      'has-error': !!validationError,
    }"
  >
    <Codemirror
      v-model="localContent"
      class="toml-editor__instance"
      :extensions="extensions"
      :autofocus="true"
      :indent-with-tab="true"
      :tab-size="2"
      :disabled="readonly"
      @change="handleChange"
    />

    <ElAlert
      v-if="validationError"
      :title="validationError"
      type="error"
      :closable="false"
      show-icon
      class="mt-3"
    />
  </div>
</template>

<script setup lang="ts">
import { Codemirror } from "vue-codemirror";
import { StreamLanguage } from "@codemirror/language";
import { toml as tom } from "@codemirror/legacy-modes/mode/toml";
import { drawSelection, EditorView } from "@codemirror/view";
interface Props {
  content: string;
  readonly?: boolean;
  validationError?: string | null;
}
const props = withDefaults(defineProps<Props>(), {
  content: "",
  readonly: false,
  validationError: null,
});
const emit = defineEmits<{
  change: [content: string];
}>();
const localContent = ref(props.content);
const extensions = [
  StreamLanguage.define(tom),
  drawSelection(),
  EditorView.lineWrapping,
  EditorView.theme({
    "&": {
      height: "420px",
      width: "100%",
      border: "1px solid var(--art-card-border)",
      borderRadius: "0",
      fontSize: "14px",
      backgroundColor: "var(--default-box-color)",
      color: "var(--art-gray-900)",
    },
    ".cm-scroller": {
      overflow: "auto",
      padding: "12px 0",
    },
    ".cm-content": {
      fontFamily:
        '"Fira Code", "SF Mono", Monaco, Menlo, "Ubuntu Mono", Consolas, monospace',
      fontSize: "14px",
      lineHeight: "1.7",
      padding: "0 16px",
      caretColor: "var(--el-color-primary, #409eff)",
    },
    ".cm-line": {
      padding: "2px 0",
    },
    ".cm-gutters": {
      minWidth: "48px",
      backgroundColor: "var(--art-gray-100)",
      border: "none",
      borderRight: "1px solid var(--art-card-border)",
      color: "var(--art-gray-500)",
    },
    ".cm-lineNumbers .cm-gutterElement": {
      padding: "0 10px 0 12px",
      fontSize: "12px",
    },
    ".cm-activeLineGutter": {
      backgroundColor: "var(--el-color-primary-light-9)",
      color: "var(--theme-color)",
    },
    ".cm-activeLine": {
      backgroundColor: "transparent",
      boxShadow: "inset 2px 0 0 var(--el-color-primary-light-5)",
    },
    ".cm-cursor, .cm-dropCursor": {
      borderLeftColor: "var(--el-color-primary, #409eff)",
      borderLeftWidth: "2px",
    },
    ".cm-selectionBackground, &.cm-focused .cm-selectionBackground, &.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground":
      {
        backgroundColor: "var(--el-color-primary-light-6) !important",
        borderRadius: "2px",
      },
    ".cm-selectionMatch": {
      backgroundColor: "var(--el-color-primary-light-9)",
    },
    ".cm-matchingBracket": {
      backgroundColor: "var(--art-gray-200)",
      color: "var(--art-gray-900)",
      borderRadius: "4px",
    },
    "&.cm-focused": {
      outline: "none",
      borderColor: "var(--el-color-primary)",
      boxShadow: "none",
    },
  }),
];
watch(
  () => props.content,
  (newContent) => {
    localContent.value = newContent;
  },
);
const handleChange = (value: string) => {
  emit("change", value);
};
</script>

<style scoped lang="scss">
.toml-editor {
  width: 100%;

  :deep(.cm-editor) {
    outline: none;
    width: 100%;
    transition:
      border-color 0.24s ease,
      box-shadow 0.24s ease;
  }

  :deep(.toml-editor__instance) {
    width: 100%;
  }

  :deep(.cm-focused) {
    outline: none;
  }

  :deep(.cm-scroller) {
    width: 100%;
    scrollbar-width: thin;
    scrollbar-color: var(--art-gray-400) transparent;
  }

  :deep(.cm-scroller::-webkit-scrollbar) {
    width: 6px;
    height: 6px;
  }

  :deep(.cm-scroller::-webkit-scrollbar-track) {
    background: transparent;
  }

  :deep(.cm-scroller::-webkit-scrollbar-thumb) {
    background: var(--art-gray-400);
    border-radius: 999px;
  }

  :deep(.cm-scroller::-webkit-scrollbar-thumb:hover) {
    background: var(--art-gray-500);
  }

  :deep(.cm-content ::selection),
  :deep(.cm-line::selection),
  :deep(.cm-line > span::selection) {
    background: var(--el-color-primary-light-7);
    color: inherit;
  }

  :deep(.cm-header) {
    color: var(--theme-color);
    font-weight: 600;
  }

  :deep(.cm-keyword) {
    color: var(--el-color-warning-dark-2);
    font-weight: 600;
  }

  :deep(.cm-string) {
    color: var(--el-color-success-dark-2);
  }

  :deep(.cm-number) {
    color: var(--el-color-danger);
  }

  :deep(.cm-comment) {
    color: var(--art-gray-500);
    font-style: italic;
  }

  :deep(.cm-def) {
    color: var(--theme-color);
    font-weight: 600;
  }

  :deep(.cm-atom) {
    color: var(--el-color-primary-dark-2);
  }

  :deep(.cm-property) {
    color: var(--art-gray-800);
  }

  :deep(.cm-operator) {
    color: var(--art-gray-600);
  }

  &.has-error {
    :deep(.cm-editor) {
      border-color: var(--el-color-danger);
      box-shadow: none;
    }
  }

  &.is-readonly {
    :deep(.cm-editor) {
      background: var(--el-fill-color-lighter);
    }

    :deep(.cm-gutters) {
      background: var(--el-fill-color-light);
    }
  }

  @media (max-width: 768px) {
    :deep(.cm-editor) {
      min-height: 320px;
      height: 320px;
    }
  }
}
</style>
