<template>
  <div class="diff-viewer">
    <div class="diff-header mb-2">
      <ElTag type="info">{{
        $t("configCenter.diffViewer.version", { version: oldVersion })
      }}</ElTag>
      <ElIcon class="mx-2"><Right /></ElIcon>
      <ElTag type="success">{{
        $t("configCenter.diffViewer.version", { version: newVersion })
      }}</ElTag>
    </div>

    <div ref="diffContainer" class="diff-container"></div>
  </div>
</template>

<script setup lang="ts">
import { $t } from "@/locales";
import { Right } from "@element-plus/icons-vue";
import * as Diff from "diff";
import { html as diff2html } from "diff2html";
import DOMPurify from "dompurify";
import "diff2html/bundles/css/diff2html.min.css";
interface Props {
  oldContent: string;
  newContent: string;
  oldVersion: number;
  newVersion: number;
}
const props = defineProps<Props>();
const diffContainer = ref<HTMLElement | null>(null);
const diffHtml = computed(() => {
  if (props.oldContent === props.newContent) {
    return `<div class="no-changes">${$t("configCenter.diffViewer.noChanges")}</div>`;
  }
  const diffPatch = Diff.createTwoFilesPatch(
    `v${props.oldVersion}.toml`,
    `v${props.newVersion}.toml`,
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
  height: 100%;
}

.diff-header {
  display: flex;
  align-items: center;
}

.diff-container {
  height: calc(100% - 32px);
  min-height: 300px;
  overflow: auto;

  border: 1px solid var(--art-card-border, var(--el-border-color-light));
  border-radius: var(--el-border-radius-base, 4px);
  background: var(--default-box-color, #fff);

  :deep(.d2h-wrapper) {
    height: 100%;
    border: none !important;
  }

  :deep(.d2h-file-wrapper) {
    border: none !important;
    margin-bottom: 0 !important;
    border-radius: 0 !important;
  }

  :deep(.d2h-file-header) {
    border: none !important;
    border-bottom: 1px solid var(--art-card-border, var(--el-border-color-light)) !important;
    background: var(--el-fill-color-light) !important;
  }

  :deep(.d2h-files-diff) {
    border: none !important;
  }

  :deep(.d2h-file-side-diff) {
    border: none !important;
  }

  :deep(.d2h-code-side-linenumber) {
    padding: 0 8px;
  }

  :deep(.d2h-file-name) {
    display: none;
  }

  .no-changes {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--el-text-color-secondary, #909399);
    font-size: 14px;
  }
}
</style>
