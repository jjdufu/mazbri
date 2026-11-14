<template>
  <div class="diff-viewer">
    <div class="diff-header mb-2">
      <ElTag type="info">{{ oldLabel }}</ElTag>
      <ElIcon class="mx-2"><Right /></ElIcon>
      <ElTag type="success">{{ newLabel }}</ElTag>
    </div>

    <div ref="diffContainer" class="diff-container"></div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { $t } from "@/locales";
import { Right } from "@element-plus/icons-vue";
import * as Diff from "diff";
import { html as diff2html } from "diff2html";
import DOMPurify from "dompurify";
import "diff2html/bundles/css/diff2html.min.css";

interface Props {
  oldContent: string;
  newContent: string;
  oldLabel: string;
  newLabel: string;
}

const props = defineProps<Props>();
const diffContainer = ref<HTMLElement | null>(null);

const diffHtml = computed(() => {
  if (props.oldContent === props.newContent) {
    return `<div class="no-changes">${$t("configCenter.diffViewer.noChanges")}</div>`;
  }
  const diffPatch = Diff.createTwoFilesPatch(
    `${props.oldLabel}.toml`,
    `${props.newLabel}.toml`,
    props.oldContent,
    props.newContent,
    "",
    "",
  );
  const rawHtml = diff2html(diffPatch, {
    drawFileList: false,
    matching: "lines",
    outputFormat: "side-by-side",
  });
  return DOMPurify.sanitize(rawHtml);
});

watch(
  diffHtml,
  (html) => {
    if (diffContainer.value) {
      diffContainer.value.innerHTML = html;
    }
  },
  { immediate: true, flush: "post" },
);
</script>

<style scoped lang="scss">
.diff-viewer {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

.diff-header {
  display: flex;
  align-items: center;
  flex-shrink: 0;
}

.diff-container {
  flex: 1;
  min-height: 280px;
  overflow: auto;
  border: 1px solid var(--el-border-color-light);
  border-radius: var(--el-border-radius-base);
  background: var(--el-bg-color);
  color: var(--el-text-color-primary);

  :deep(.d2h-wrapper) {
    height: 100%;
    border: none !important;
    background: var(--el-bg-color);
    color: var(--el-text-color-primary);
  }

  :deep(.d2h-file-wrapper) {
    border: none !important;
    margin-bottom: 0 !important;
    border-radius: 0 !important;
    background: var(--el-bg-color);
  }

  :deep(.d2h-file-header) {
    display: none;
  }

  :deep(.d2h-files-diff),
  :deep(.d2h-file-side-diff),
  :deep(.d2h-diff-table),
  :deep(.d2h-diff-tbody) {
    border: none !important;
    background: var(--el-bg-color);
    color: var(--el-text-color-primary);
  }

  :deep(.d2h-code-side-linenumber) {
    padding: 0 8px;
    background: var(--el-fill-color-light);
    border-color: var(--el-border-color-lighter);
    color: var(--el-text-color-secondary);
  }

  :deep(.d2h-code-side-line) {
    color: var(--el-text-color-primary);
  }

  :deep(.d2h-cntx) {
    background: var(--el-bg-color);
    color: var(--el-text-color-primary);
  }

  :deep(.d2h-emptyplaceholder),
  :deep(.d2h-code-side-emptyplaceholder) {
    background: var(--el-fill-color-lighter);
    border-color: var(--el-border-color-lighter);
  }

  :deep(.d2h-info) {
    background: var(--el-fill-color-light);
    border-color: var(--el-border-color-lighter);
    color: var(--el-text-color-secondary);
  }

  :deep(.d2h-del) {
    background: var(--el-color-danger-light-9);
    border-color: var(--el-color-danger-light-7);
  }

  :deep(.d2h-ins) {
    background: var(--el-color-success-light-9);
    border-color: var(--el-color-success-light-7);
  }

  :deep(.d2h-code-line del) {
    background: var(--el-color-danger-light-7);
    color: var(--el-text-color-primary);
  }

  :deep(.d2h-code-line ins) {
    background: var(--el-color-success-light-7);
    color: var(--el-text-color-primary);
  }

  :deep(.no-changes) {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    min-height: 240px;
    color: var(--el-text-color-secondary);
    font-size: 14px;
  }
}
</style>
