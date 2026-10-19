<template>
  <EditDialog
    v-model="visible"
    :title="title"
    width="700px"
    @close="handleClose"
  >
    <div v-if="step === 1">
      <FilePicker
        ref="filePickerRef"
        :model-value="file"
        accept=".csv,text/csv"
        :max-size="maxFileSize"
        :title="
          $t('common.importDialog.dragToUpload', { type: fileTypeLabel })
        "
        :hint="$t('common.importDialog.fileTip', { ext: 'csv', size: '100KB' })"
        :file-too-large-message="$t('feedback.validation.fileSizeLimit100kb')"
        empty-icon="ri:upload-cloud-2-line"
        selected-icon="ri:file-excel-2-line"
        @update:model-value="handleFileUpdate"
      />
      <p v-if="formatHint" class="import-dialog__format-hint">
        {{ formatHint }}
      </p>
    </div>

    <div v-else-if="step === 2">
      <ElAlert
        v-if="preview.new.length > 0"
        type="success"
        :closable="false"
        class="mb-3"
      >
        <template #title>
          {{
            $t("common.importDialog.newItems", { count: preview.new.length })
          }}
        </template>
        {{ $t("common.importDialog.willBeAdded") }}
      </ElAlert>

      <ElAlert
        v-if="preview.duplicate.length > 0"
        type="warning"
        :closable="false"
        class="mb-3"
      >
        <template #title>
          {{
            $t("common.importDialog.duplicateItems", {
              count: preview.duplicate.length,
            })
          }}
        </template>
        {{ $t("common.importDialog.alreadyExists") }}
      </ElAlert>

      <ElAlert
        v-if="preview.invalid.length > 0"
        type="error"
        :closable="false"
        class="mb-4"
      >
        <template #title>
          {{
            $t("common.importDialog.invalidItems", {
              count: preview.invalid.length,
            })
          }}
        </template>
        <div
          v-for="(item, index) in preview.invalid.slice(0, 5)"
          :key="index"
          class="invalid-item"
        >
          {{ item.raw }} - {{ item.reason }}
        </div>
        <div v-if="preview.invalid.length > 5">
          {{
            $t("common.importDialog.moreItems", {
              count: preview.invalid.length - 5,
            })
          }}
        </div>
      </ElAlert>

      <div v-if="preview.new.length > 0" class="preview-section">
        <div class="preview-title">
          {{ $t("common.importDialog.previewTitle", { count: previewLimit }) }}
        </div>
        <ArtTable
          :data="preview.new.slice(0, previewLimit)"
          border
          size="small"
          max-height="300"
        >
          <ElTableColumn
            v-for="col in columns"
            :key="col.key"
            :prop="col.key"
            :label="col.label"
            :min-width="col.key === 'description' ? 200 : 150"
          >
            <template v-if="col.type === 'array'" #default="{ row }">
              <ElTag
                v-for="(item, idx) in (row[col.key] as string[])?.slice(0, 3)"
                :key="idx"
                size="small"
                class="mr-1"
              >
                {{ item }}
              </ElTag>
              <ElTag
                v-if="(row[col.key] as string[])?.length > 3"
                size="small"
                type="info"
              >
                +{{ (row[col.key] as string[]).length - 3 }}
              </ElTag>
            </template>
          </ElTableColumn>
        </ArtTable>
      </div>
    </div>

    <template #footer>
      <ElButton @click="() => handleClose()">
        {{ step === 1 ? $t("common.cancel") : $t("common.importDialog.back") }}
      </ElButton>
      <ElButton
        v-if="step === 1"
        type="primary"
        :disabled="!file"
        @click="handlePreview"
      >
        {{ $t("common.importDialog.nextStep") }}
      </ElButton>
      <ElButton
        v-if="step === 2"
        type="primary"
        :loading="importing"
        :disabled="preview.new.length === 0"
        @click="handleConfirm"
      >
        {{ $t("common.importDialog.confirmImport") }}
      </ElButton>
    </template>
  </EditDialog>
</template>

<script setup lang="ts">
import { ElMessage } from "element-plus";
import { $t } from "@/locales";
import type {
  ColumnConfig,
  PreviewResult,
} from "@/composables/useResourceCsvImportExport";
import {
  hasResourceCsvImportRows,
  parseResourceCsvImportPreview,
} from "@/composables/useResourceCsvImportExport";
import { ErrorCodes } from "@/constants/error-codes";
import EditDialog from "@/components/common/EditDialog.vue";
import FilePicker from "@/components/common/file-picker/index.vue";
import { logger } from "@/utils/sys";

interface Props {
  modelValue: boolean;
  title: string;
  columns: ColumnConfig[];

  formatHint?: string;
  uniqueKey: string;
  existingData: Record<string, unknown>[];
  validator?: (row: Record<string, string>) => {
    valid: boolean;
    reason?: string;
  };
  transformer: (row: Record<string, string>) => Record<string, unknown>;
  onImport: (items: Record<string, unknown>[]) => Promise<void>;
  showSuccessMessage?: boolean;
  showErrorMessage?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  formatHint: "",
});
const emit = defineEmits<{
  "update:modelValue": [value: boolean];
  success: [];
}>();

const visible = computed({
  get: () => props.modelValue,
  set: (val) => emit("update:modelValue", val),
});

const showSuccessMessage = computed(() => props.showSuccessMessage !== false);
const showErrorMessage = computed(() => props.showErrorMessage !== false);
const filePickerRef = ref<{ clear: () => void } | null>(null);
const file = ref<File | null>(null);
const step = ref(1);
const importing = ref(false);
const previewLimit = 10;
const maxFileSize = 100 * 1024;
const fileTypeLabel = "CSV";
const preview = ref<PreviewResult<Record<string, unknown>>>({
  new: [],
  duplicate: [],
  invalid: [],
});

function handleFileUpdate(value: File | File[] | null) {
  if (value instanceof File) {
    file.value = value;
    return;
  }
  if (Array.isArray(value)) {
    file.value = value[0] ?? null;
    return;
  }
  file.value = null;
}

const handlePreview = () => {
  if (!file.value) return;
  const reader = new FileReader();
  reader.onload = (e) => {
    try {
      const content = e.target?.result as string;
      if (!hasResourceCsvImportRows(content)) {
        ElMessage.error($t("feedback.validation.csvNeedHeaderAndRow"));
        preview.value = { new: [], duplicate: [], invalid: [] };
        return;
      }
      const result = parseResourceCsvImportPreview({
        content,
        columns: props.columns,
        validator: props.validator,
        transformer: props.transformer,
        uniqueKey: props.uniqueKey,
        existingData: props.existingData,
      });
      const missingColumn =
        result.invalid.length === 1 &&
        result.new.length === 0 &&
        result.duplicate.length === 0 &&
        result.invalid[0].raw.trim() ===
          content.trim().split(/\r?\n/)[0]?.trim();
      if (missingColumn) {
        ElMessage.error(result.invalid[0].reason);
        preview.value = { new: [], duplicate: [], invalid: [] };
        return;
      }
      preview.value = result;
      step.value = 2;
      if (
        result.new.length === 0 &&
        result.invalid.length === 0 &&
        result.duplicate.length === 0
      ) {
        ElMessage.warning($t("feedback.validation.csvNoValidData"));
      }
    } catch (error) {
      logger.error("CSV parse error", {
        service: "mazbri-mgmt-web",
        event: "import_dialog_csv_parse_failed",
        severity: "error",
        error_code: ErrorCodes.INTERNAL_ERROR,
        error,
      });
      ElMessage.error($t("feedback.validation.csvParseFailed"));
      preview.value = { new: [], duplicate: [], invalid: [] };
    }
  };
  reader.readAsText(file.value);
};

const handleConfirm = async () => {
  if (preview.value.new.length === 0) return;
  importing.value = true;
  try {
    await props.onImport(preview.value.new);
    if (showSuccessMessage.value) {
      ElMessage.success(
        $t("feedback.import.importCountSuccess", {
          count: preview.value.new.length,
        }),
      );
    }
    emit("success");
    handleClose(true);
  } catch (error) {
    logger.error("Import failed", {
      service: "mazbri-mgmt-web",
      event: "import_dialog_import_failed",
      severity: "error",
      error_code: ErrorCodes.INTERNAL_ERROR,
      error,
    });
    if (showErrorMessage.value) {
      ElMessage.error($t("feedback.import.importFailed"));
    }
  } finally {
    importing.value = false;
  }
};

const handleClose = (completed = false) => {
  if (step.value === 2 && !completed) {
    step.value = 1;
    preview.value = { new: [], duplicate: [], invalid: [] };
  } else {
    visible.value = false;
    file.value = null;
    preview.value = { new: [], duplicate: [], invalid: [] };
    step.value = 1;
    filePickerRef.value?.clear();
  }
};
</script>

<style scoped lang="scss">
.preview-section {
  margin-top: 16px;
}

.preview-title {
  font-size: 14px;
  font-weight: 500;
  margin-bottom: 8px;
  color: var(--el-text-color-primary);
}

.invalid-item {
  font-size: 12px;
  color: var(--el-color-danger);
  margin-top: 4px;
  font-family: "SF Mono", "Monaco", "Inconsolata", monospace;
}

.import-dialog__format-hint {
  margin-top: 12px;
  white-space: pre-line;
  font-size: 12px;
  color: var(--el-text-color-secondary);
  line-height: 1.5;
}

.mb-3 {
  margin-bottom: 12px;
}

.mb-4 {
  margin-bottom: 16px;
}
</style>
