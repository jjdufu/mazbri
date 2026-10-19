<template>
  <div class="json-template-editor">
    <div class="editor-header">
      <ElRadioGroup v-model="mode" size="small" :disabled="disabled" @change="handleModeChange">
        <ElRadioButton label="form" :disabled="disabled || !canUseForm"
          >表单配置</ElRadioButton
        >
        <ElRadioButton label="raw" :disabled="disabled">源码编辑</ElRadioButton>
      </ElRadioGroup>

      <div class="header-actions">
        <ElTooltip
          v-if="!canUseForm"
          content="含复杂模板逻辑，已切换源码模式"
          placement="top"
        >
          <span class="warning-tip mr-2">
            <ArtSvgIcon icon="ri:alert-line" class="mr-1" />
            仅限源码模式
          </span>
        </ElTooltip>

        <template v-if="mode === 'raw'">
          <ElButton link type="primary" size="small" :disabled="disabled" @click="formatJson">
            格式化 JSON
          </ElButton>
        </template>
      </div>
    </div>

    <div v-if="mode === 'form'" class="form-editor-container">
      <ElTable
        v-if="showFormTable"
        :data="formItems"
        stripe
        class="form-kv-table"
      >
        <ElTableColumn :label="keyColumnLabel" min-width="150">
          <template #default="scope">
            <ElInput
              v-model="scope.row.key"
              :placeholder="keyPlaceholder"
              :disabled="disabled"
              @input="serializeForm"
            />
          </template>
        </ElTableColumn>
        <ElTableColumn :label="valueColumnLabel" min-width="250">
          <template #default="scope">
            <ElInput
              v-model="scope.row.value"
              :placeholder="valuePlaceholder"
              :disabled="disabled"
              @input="serializeForm"
            >
              <template #suffix>
                <ElDropdown
                  trigger="click"
                  :disabled="disabled"
                  @command="
                    (command: string) =>
                      insertVariableToRow(scope.$index, command)
                  "
                >
                  <span
                    :class="[
                      'var-btn-trigger',
                      { 'is-disabled': disabled }
                    ]"
                  >
                    <ArtSvgIcon icon="ri:add-circle-line" class="mr-0.5" />
                    变量
                  </span>
                  <template #dropdown>
                    <ElDropdownMenu>
                      <ElDropdownItem
                        v-for="v in variables"
                        :key="v"
                        :command="v"
                      >
                        <span v-text="'{{ ' + v + ' }}'" />
                      </ElDropdownItem>
                    </ElDropdownMenu>
                  </template>
                </ElDropdown>
              </template>
            </ElInput>
          </template>
        </ElTableColumn>
        <ElTableColumn :label="opColumnLabel" width="60" align="center">
          <template #default="scope">
            <ElButton
              type="danger"
              link
              :disabled="disabled"
              @click="removeFormItem(scope.$index)"
            >
              <ArtSvgIcon icon="ri:delete-bin-line" />
            </ElButton>
          </template>
        </ElTableColumn>
      </ElTable>
      <div class="form-add-row">
        <ElButton link type="primary" size="small" :disabled="disabled" @click="addFormItem">
          <ArtSvgIcon icon="ri:add-circle-line" class="mr-1" />
          添加字段
        </ElButton>
      </div>
    </div>

    <div v-else class="raw-editor-container">
      <Codemirror
        v-model="rawContent"
        class="cm-editor-instance"
        :extensions="extensions"
        :autofocus="false"
        :indent-with-tab="true"
        :tab-size="2"
        placeholder="{}"
        @change="handleRawChange"
      />
    </div>

    <ElAlert
      v-if="validationError"
      :title="validationError"
      type="error"
      :closable="false"
      show-icon
      class="editor-validation-alert"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, watch, onMounted, computed } from "vue";
import { Codemirror } from "vue-codemirror";
import { StreamLanguage } from "@codemirror/language";
import { javascript } from "@codemirror/legacy-modes/mode/javascript";
import { drawSelection, EditorView } from "@codemirror/view";
import { EditorState } from "@codemirror/state";

interface Props {
  modelValue: string;
  variables?: string[];
  mode?: "form" | "raw";
  disabled?: boolean;
  height?: string;
  emptyPlaceholder?: boolean;
  keyColumnLabel?: string;
  valueColumnLabel?: string;
  opColumnLabel?: string;
  keyPlaceholder?: string;
  valuePlaceholder?: string;
}

const props = withDefaults(defineProps<Props>(), {
  modelValue: "",
  variables: () => [],
  mode: "form",
  disabled: false,
  height: "180px",
  emptyPlaceholder: false,
  keyColumnLabel: "参数名 (Key)",
  valueColumnLabel: "参数值 (Value)",
  opColumnLabel: "操作",
  keyPlaceholder: "例如: username",
  valuePlaceholder: "输入值，或右侧插入变量",
});

const emit = defineEmits<{
  (e: "update:modelValue", value: string): void;
  (e: "update:mode", value: "form" | "raw"): void;
}>();

const mode = ref<"form" | "raw">(props.mode);
const canUseForm = ref(true);
const formItems = ref<{ key: string; value: string }[]>([]);
const rawContent = ref("");
const validationError = ref<string | null>(null);

const hasFormFields = computed(() =>
  formItems.value.some((item) => item.key.trim() || item.value.trim()),
);

const showFormTable = computed(() => {
  if (!props.emptyPlaceholder) return true;
  return formItems.value.length > 0;
});

const editorHeight = computed(() => {
  if (!props.emptyPlaceholder) return props.height;
  if (mode.value === "form" && !hasFormFields.value) return "0px";
  return props.height && props.height !== "180px" ? props.height : "80px";
});

const extensions = computed(() => [
  StreamLanguage.define(javascript),
  drawSelection(),
  EditorView.lineWrapping,
  EditorView.theme({
    "&": {
      height: editorHeight.value,
      width: "100%",

      border: "none",
      borderRadius: "0",
      outline: "none",
      fontSize: "13px",
      backgroundColor: props.disabled ? "var(--el-fill-color-light)" : "var(--el-fill-color-blank)",
    },
    "&.cm-focused": {
      outline: "none",
    },
    ".cm-scroller": {
      overflow: "auto",
    },
    ".cm-content": {
      fontFamily: '"Fira Code", "SF Mono", Monaco, Menlo, monospace',
      lineHeight: "1.6",
      padding: "8px 12px",
    },
    ".cm-gutters": {
      backgroundColor: "var(--el-fill-color-light)",
      borderRight: "1px solid var(--el-border-color-light)",
      color: "var(--el-text-color-placeholder)",
    },
  }),
  EditorState.readOnly.of(props.disabled),
  EditorView.editable.of(!props.disabled),
]);

function parseLenientJsonTemplate(
  str: string,
): { key: string; value: string }[] | null {
  const trimmed = str.trim();
  if (!trimmed) return [];
  if (!trimmed.startsWith("{") || !trimmed.endsWith("}")) return null;

  const inner = trimmed.slice(1, -1).trim();
  if (!inner) return [];

  const items: { key: string; value: string }[] = [];
  let i = 0;

  try {
    while (i < inner.length) {

      while (
        i < inner.length &&
        (inner[i] === " " ||
          inner[i] === "\t" ||
          inner[i] === "\n" ||
          inner[i] === "\r" ||
          inner[i] === ",")
      ) {
        i++;
      }
      if (i >= inner.length) break;

      if (inner[i] !== '"') return null;
      i++;
      const keyStart = i;
      while (i < inner.length && inner[i] !== '"') {
        if (inner[i] === "\\") i++;
        i++;
      }
      if (i >= inner.length) return null;
      const key = inner.slice(keyStart, i);
      i++;

      while (
        i < inner.length &&
        (inner[i] === " " ||
          inner[i] === "\t" ||
          inner[i] === "\n" ||
          inner[i] === "\r")
      ) {
        i++;
      }
      if (inner[i] !== ":") return null;
      i++;

      while (
        i < inner.length &&
        (inner[i] === " " ||
          inner[i] === "\t" ||
          inner[i] === "\n" ||
          inner[i] === "\r")
      ) {
        i++;
      }
      if (i >= inner.length) return null;

      if (inner[i] === '"') {

        i++;
        const strStart = i;
        while (i < inner.length && inner[i] !== '"') {
          if (inner[i] === "\\") i++;
          i++;
        }
        if (i >= inner.length) return null;
        const val = inner.slice(strStart, i);
        items.push({ key, value: val });
        i++;
      } else if (inner[i] === "{" && inner[i + 1] === "{") {

        let bracesCount = 0;
        const exprStart = i;
        while (i < inner.length) {
          if (inner[i] === "{" && inner[i + 1] === "{") {
            bracesCount++;
            i += 2;
          } else if (inner[i] === "}" && inner[i + 1] === "}") {
            bracesCount--;
            i += 2;
            if (bracesCount === 0) break;
          } else {
            i++;
          }
        }
        const val = inner.slice(exprStart, i).trim();
        items.push({ key, value: val });
      } else {

        let braceCount = 0;
        let bracketCount = 0;
        const valStart = i;
        while (i < inner.length) {
          const char = inner[i];
          if (char === "{") braceCount++;
          else if (char === "}") braceCount--;
          else if (char === "[") bracketCount++;
          else if (char === "]") bracketCount--;
          else if (char === "," && braceCount === 0 && bracketCount === 0)
            break;
          i++;
        }
        const val = inner.slice(valStart, i).trim();

        if (
          (val.startsWith("{") && !val.startsWith("{{")) ||
          val.startsWith("[")
        ) {
          return null;
        }
        items.push({ key, value: val });
      }
    }
    return items;
  } catch {
    return null;
  }
}

const serializeForm = () => {
  const lines = formItems.value
    .filter((item) => item.key.trim() !== "")
    .map((item) => {
      const k = JSON.stringify(item.key);
      const v = item.value.trim();

      const isRawExpr = v.startsWith("{{") && v.endsWith("}}");
      const isNested =
        (v.startsWith("{") && v.endsWith("}")) ||
        (v.startsWith("[") && v.endsWith("]"));
      const isNumber = !isNaN(Number(v)) && v !== "";
      const isBoolOrNull = ["true", "false", "null"].includes(v.toLowerCase());

      if (isRawExpr || isNested || isNumber || isBoolOrNull) {
        return `  ${k}: ${v}`;
      } else {
        return `  ${k}: ${JSON.stringify(v)}`;
      }
    });

  const serialized = `{\n${lines.join(",\n")}\n}`;
  rawContent.value = serialized;
  validationError.value = null;
  emit("update:modelValue", serialized);
};

const initFromModel = () => {
  const val = props.modelValue || "";
  rawContent.value = val;

  if (!val.trim()) {
    formItems.value = props.emptyPlaceholder ? [] : [{ key: "", value: "" }];
    canUseForm.value = true;
    validationError.value = null;
    return;
  }

  try {
    const parsed = JSON.parse(val);
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      formItems.value = Object.entries(parsed).map(([k, v]) => ({
        key: k,
        value: typeof v === "object" ? JSON.stringify(v) : String(v),
      }));
      canUseForm.value = true;
      validationError.value = null;
      return;
    }
  } catch {
    void 0;
  }

  const parsedItems = parseLenientJsonTemplate(val);
  if (parsedItems) {
    formItems.value = parsedItems;
    canUseForm.value = true;
    validationError.value = null;
  } else {
    canUseForm.value = false;
    mode.value = "raw";
    emit("update:mode", "raw");
    validateRawJinjaJson(val);
  }
};

const validateRawJinjaJson = (str: string) => {
  if (!str.trim()) {
    validationError.value = null;
    return;
  }
  let openBraces = 0;
  for (const c of str) {
    if (c === "{") openBraces++;
    else if (c === "}") openBraces--;
  }
  if (openBraces !== 0) {
    validationError.value = "JSON 大括号不匹配，请检查语法结构";
  } else {
    validationError.value = null;
  }
};

const handleModeChange = (val?: string | number | boolean) => {
  const newMode = val as "form" | "raw";
  emit("update:mode", newMode);
  if (newMode === "form") {
    initFromModel();
  }
};

const handleRawChange = (val: string) => {
  validateRawJinjaJson(val);
  emit("update:modelValue", val);
};

const formatJson = () => {
  const val = rawContent.value;
  try {
    const parsed = JSON.parse(val);
    const formatted = JSON.stringify(parsed, null, 2);
    rawContent.value = formatted;
    emit("update:modelValue", formatted);
    validationError.value = null;
  } catch {
    const parsedItems = parseLenientJsonTemplate(val);
    if (parsedItems) {
      formItems.value = parsedItems;
      serializeForm();
    } else {
      validationError.value =
        "格式化失败：JSON 语法有误或含无法格式化的模板";
    }
  }
};

const addFormItem = () => {
  formItems.value.push({ key: "", value: "" });
};

const removeFormItem = (index: number) => {
  formItems.value.splice(index, 1);
  if (formItems.value.length === 0 && !props.emptyPlaceholder) {
    formItems.value.push({ key: "", value: "" });
  }
  serializeForm();
};

const insertVariableToRow = (index: number, varName: string) => {
  const item = formItems.value[index];
  item.value = (item.value || "") + `{{ ${varName} }}`;
  serializeForm();
};

const insertVarRaw = (varName: string) => {
  rawContent.value = (rawContent.value || "") + `{{ ${varName} }}`;
  emit("update:modelValue", rawContent.value);
  validateRawJinjaJson(rawContent.value);
};

defineExpose({
  insertVarRaw,
  mode,
  canUseForm,
  addFormItem,
});

watch(
  () => props.modelValue,
  (newVal) => {
    if (newVal !== rawContent.value) {
      initFromModel();
    }
  },
);

watch(
  () => props.mode,
  (newMode) => {
    if (newMode && newMode !== mode.value) {
      mode.value = newMode;
      initFromModel();
    }
  },
);

onMounted(() => {
  initFromModel();
});
</script>

<style scoped lang="scss">
.json-template-editor {
  border: 1px solid var(--el-border-color-light);
  border-radius: var(--el-border-radius-base);
  background-color: var(--el-fill-color-blank);
  overflow: hidden;
  width: 100%;
  box-shadow: var(--el-box-shadow-light) 0 2px 12px 0;

  .editor-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 6px 12px;
    background-color: var(--el-fill-color-light);
    border-bottom: 1px solid var(--el-border-color-light);

    .header-actions {
      display: flex;
      align-items: center;
    }

    .warning-tip {
      font-size: 11px;
      color: var(--el-color-warning);
      display: inline-flex;
      align-items: center;
    }
  }

  .form-add-row {
    padding: 6px 12px 8px;
    border-top: 1px solid var(--el-border-color-extra-light);
  }

  .form-editor-container {
    padding: 0;

    .form-kv-table {
      width: 100%;
      margin-bottom: 0;
      border: none !important;
      border-radius: 0;

      :deep(.el-table__inner-wrapper) {
        border-radius: inherit;
        overflow: hidden;
      }

      :deep(.el-table__inner-wrapper::before) {
        display: none;
      }

      :deep(.el-table__border-left-patch) {
        display: none;
      }

      :deep(.el-input__wrapper) {
        box-shadow: none !important;
        background-color: transparent;
      }
    }

    .var-btn-trigger {
      cursor: pointer;
      color: var(--el-color-primary);
      font-size: 12px;
      display: inline-flex;
      align-items: center;
      padding: 2px 6px;
      background-color: var(--el-color-primary-light-9);
      border-radius: 3px;
      transition: background-color 0.2s;

      &:hover {
        background-color: var(--el-color-primary-light-8);
      }

      &.is-disabled {
        cursor: not-allowed !important;
        color: var(--el-text-color-placeholder) !important;
        background-color: var(--el-fill-color-light) !important;
        pointer-events: none;
      }
    }
  }

  .raw-editor-container {
    width: 100%;


    :deep(.cm-editor),
    :deep(.cm-editor.cm-focused) {
      border: none !important;
      outline: none !important;
      border-radius: 0 !important;
    }
  }

  .editor-validation-alert {
    border-top: 1px solid var(--el-border-color-light);
    border-radius: 0;
    font-size: 12px;
  }
}
</style>
