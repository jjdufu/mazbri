<template>
  <div class="wb-code" :class="{ 'is-fill': fill, 'is-readonly': readonly }">
    <div v-if="showToolbar" class="wb-code__bar">
      <span class="wb-code__lang">{{ displayLang }}</span>
      <div class="wb-code__actions">
        <button
          v-if="!readonly && isJsonLang"
          type="button"
          class="wb-code__btn"
          :title="$t('upstreamConnector.formatJson')"
          @click="format"
        >
          <ArtSvgIcon icon="ri:magic-line" />
          <span>{{ $t("upstreamConnector.formatShort") }}</span>
        </button>
        <button
          type="button"
          class="wb-code__btn"
          :title="$t('common.actions.copy')"
          @click="copy"
        >
          <ArtSvgIcon icon="ri:file-copy-line" />
          <span>{{ $t("common.actions.copy") }}</span>
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
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { json } from "@codemirror/lang-json";
import { xml } from "@codemirror/lang-xml";
import {
  HighlightStyle,
  bracketMatching,
  syntaxHighlighting,
} from "@codemirror/language";
import { EditorState, type Extension } from "@codemirror/state";
import {
  EditorView,
  drawSelection,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
} from "@codemirror/view";
import { tags } from "@lezer/highlight";
import { $t } from "@/locales";
import { useSettingStore } from "@/store/modules/setting";
import { copyWithFeedback } from "@/utils/ui/copy-feedback";
import { prettyJson } from "./workbenchHelpers";

const props = withDefaults(
  defineProps<{
    modelValue: string;
    readonly?: boolean;
    fill?: boolean;
    height?: string;
    showToolbar?: boolean;
    placeholder?: string;
    langLabel?: "JSON" | "XML" | "GraphQL" | "TEXT" | string;
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

const settingStore = useSettingStore();
const local = ref(props.modelValue || "");

watch(
  () => props.modelValue,
  (v) => {
    if (v !== local.value) local.value = v || "";
  },
);

const normalizedLang = computed(() =>
  (props.langLabel || "JSON").trim().toUpperCase(),
);

const isJsonLang = computed(() => normalizedLang.value === "JSON");

const displayLang = computed(() => {
  if (normalizedLang.value === "TEXT") return "RAW TEXT";
  return props.langLabel || "JSON";
});

function languageExtension(): Extension {
  if (normalizedLang.value === "JSON") return json();
  if (normalizedLang.value === "XML") return xml();
  return [];
}

function buildSyntaxHighlight(dark: boolean): Extension {
  const style = HighlightStyle.define(
    dark
      ? [
          { tag: tags.propertyName, color: "#79c0ff" },
          { tag: tags.string, color: "#7ee787" },
          { tag: tags.number, color: "#ffa657" },
          { tag: tags.bool, color: "#d2a8ff" },
          { tag: tags.null, color: "#ff7b72" },
          { tag: tags.tagName, color: "#7ee787" },
          { tag: tags.attributeName, color: "#79c0ff" },
          { tag: tags.attributeValue, color: "#a5d6ff" },
          { tag: tags.keyword, color: "#ff7b72" },
          { tag: tags.punctuation, color: "#a1a1aa" },
        ]
      : [
          { tag: tags.propertyName, color: "#0550ae" },
          { tag: tags.string, color: "#116329" },
          { tag: tags.number, color: "#953800" },
          { tag: tags.bool, color: "#8250df" },
          { tag: tags.null, color: "#cf222e" },
          { tag: tags.tagName, color: "#116329" },
          { tag: tags.attributeName, color: "#0550ae" },
          { tag: tags.attributeValue, color: "#0a3069" },
          { tag: tags.keyword, color: "#cf222e" },
          { tag: tags.punctuation, color: "#71717a" },
        ],
  );
  return syntaxHighlighting(style);
}

const extensions = computed<Extension[]>(() => {
  const dark = Boolean(settingStore.isDark);
  return [
    lineNumbers(),
    history(),
    bracketMatching(),
    drawSelection(),
    highlightActiveLine(),
    highlightActiveLineGutter(),
    keymap.of([...defaultKeymap, ...historyKeymap]),
    languageExtension(),
    buildSyntaxHighlight(dark),
    EditorView.lineWrapping,
    EditorView.theme(
      {
        "&": {
          height: props.fill ? "100%" : props.height,
          width: "100%",
          border: "none",
          outline: "none",
          fontSize: "12.5px",
          backgroundColor: "var(--wb-surface, #ffffff)",
          color: "var(--wb-text, #111827)",
        },
        "&.cm-focused": { outline: "none" },
        "&.cm-focused .cm-cursor": {
          borderLeftColor: "var(--wb-primary, #5d87ff)",
        },
        "&.cm-focused .cm-selectionBackground, .cm-selectionBackground": {
          backgroundColor:
            "color-mix(in srgb, var(--el-color-primary, #5d87ff) 22%, transparent) !important",
        },
        ".cm-scroller": {
          overflow: "auto",
          fontFamily:
            "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace",
          lineHeight: "1.6",
        },
        ".cm-content": {
          padding: "8px 0",
          caretColor: "var(--wb-primary, #5d87ff)",
        },
        ".cm-line": {
          padding: "0 12px",
        },
        ".cm-gutters": {
          backgroundColor: "var(--wb-surface-subtle, #f6f8fc)",
          color: "var(--wb-text-faint, #8b95a7)",
          border: "none",
          borderRight: "1px solid var(--wb-divider, #dfe5f0)",
          minWidth: "38px",
        },
        ".cm-activeLine": {
          backgroundColor:
            "color-mix(in srgb, var(--el-color-primary, #5d87ff) 6%, transparent)",
        },
        ".cm-activeLineGutter": {
          backgroundColor:
            "color-mix(in srgb, var(--el-color-primary, #5d87ff) 10%, transparent)",
          color: "var(--wb-primary, #5d87ff)",
        },
      },
      { dark },
    ),
    EditorState.readOnly.of(props.readonly),
    EditorView.editable.of(!props.readonly),
  ];
});

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
  await copyWithFeedback(local.value || "");
}
</script>

<style scoped lang="scss">
.wb-code {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
  background: var(--wb-surface, #fff);
  border: 1px solid var(--wb-divider, #dfe5f0);
  border-radius: var(--wb-radius, 4px);
}

.wb-code.is-fill {
  flex: 1;
  height: 100%;
}

.wb-code__bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  min-height: 30px;
  padding: 2px 8px 2px 12px;
  background: var(--wb-surface-subtle, #f6f8fc);
  border-bottom: 1px solid var(--wb-divider, #dfe5f0);
}

.wb-code__lang {
  font-family: var(--wb-font-mono);
  font-size: 11px;
  font-weight: 600;
  color: var(--wb-text-muted, #4b5565);
  letter-spacing: 0.04em;
}

.wb-code__actions {
  display: flex;
  gap: 4px;
  align-items: center;
}

.wb-code__btn {
  display: inline-flex;
  gap: 4px;
  align-items: center;
  height: 24px;
  padding: 0 8px;
  font-size: 11px;
  font-weight: 500;
  color: var(--wb-text-muted, #4b5565);
  cursor: pointer;
  background: transparent;
  border: none;
  border-radius: var(--wb-radius, 4px);

  &:hover {
    color: var(--wb-text, #111827);
    background: var(--wb-surface-hover, #eaeff8);
  }
}

.wb-code__cm {
  flex: 1;
  min-height: 0;
}

.wb-code :deep(.cm-editor) {
  height: 100%;
}
</style>
