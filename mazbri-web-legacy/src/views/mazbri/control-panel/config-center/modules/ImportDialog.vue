<template>
  <EditDialog
    v-model="visible"
    :title="$t('configCenter.importDialog.title')"
    width="700px"
    :confirm-text="$t('configCenter.importDialog.confirm')"
    :cancel-text="$t('common.cancel')"
    :saving="importing"
    :confirm-disabled="authorizationDisabled || !canImport"
    :confirm-disabled-reason="
      authorizationDisabled ? authorizationDisabledReason : undefined
    "
    @confirm="handleImport"
  >
    <FilePicker
      :model-value="files"
      multiple
      accept=".toml,.zip"
      :disabled="authorizationDisabled"
      :title="
        $t('configCenter.importDialog.uploadText') +
        $t('configCenter.importDialog.clickToUpload')
      "
      :hint="$t('configCenter.importDialog.tip')"
      :action-label="$t('configCenter.importDialog.clickToUpload')"
      :invalid-type-message="
        $t('configCenter.importDialog.validation.fileType')
      "
      empty-icon="ri:upload-cloud-2-line"
      selected-icon="ri:file-code-line"
      @update:model-value="handleFilesUpdate"
    />
  </EditDialog>
</template>

<script setup lang="ts">
import { ElMessage } from "element-plus";
import { $t } from "@/locales";
import { importConfigs } from "@/api/config-center";
import EditDialog from "@/components/common/EditDialog.vue";
import FilePicker from "@/components/common/file-picker/index.vue";
import { getLicenseDisabledReason } from "@/composables/useLicenseGate";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-dialog-service";
import { logger } from "@/utils/sys";

interface Props {
  modelValue: boolean;
  authorizationDisabled?: boolean;
  authorizationDisabledReason?: string;
}

const props = defineProps<Props>();
const emit = defineEmits<{
  "update:modelValue": [value: boolean];
  success: [];
}>();

const visible = computed({
  get: () => props.modelValue,
  set: (val) => emit("update:modelValue", val),
});

const files = ref<File[]>([]);
const importing = ref(false);

const authorizationDisabled = computed(
  () => props.authorizationDisabled ?? false,
);
const authorizationDisabledReason = computed(
  () =>
    props.authorizationDisabledReason ?? getLicenseDisabledReason(),
);
const canImport = computed(() => files.value.length > 0);

function handleFilesUpdate(value: File | File[] | null) {
  if (Array.isArray(value)) {
    files.value = value;
    return;
  }
  if (value instanceof File) {
    files.value = [value];
    return;
  }
  files.value = [];
}

const handleImport = async () => {
  if (authorizationDisabled.value) {
    ElMessage.warning(authorizationDisabledReason.value);
    throw new Error(authorizationDisabledReason.value);
  }
  if (files.value.length === 0) return;
  importing.value = true;
  try {
    const result = await importConfigs(files.value);
    const imported = result.imported;
    const failed = result.skipped;
    const errors = result.errors ?? [];
    const parts: string[] = [];
    if (imported > 0) {
      parts.push(
        $t("configCenter.importDialog.messages.importedCount", {
          count: imported,
        }),
      );
    }
    if (failed > 0) {
      parts.push(
        $t("configCenter.importDialog.messages.failedCount", { count: failed }),
      );
    }
    if (errors.length > 0) {
      logger.error("Import errors:", errors);
    }
    const summary = $t("configCenter.importDialog.messages.importCompleted", {
      details: parts.join("，"),
    });
    if (failed > 0 && imported === 0) {
      ElMessage.warning(errors[0] ? `${summary}（${errors[0]}）` : summary);
      return;
    }
    if (failed > 0) {
      ElMessage.warning(errors[0] ? `${summary}（${errors[0]}）` : summary);
    } else {
      ElMessage.success(summary);
    }
    visible.value = false;
    emit("success");
  } catch (error: unknown) {
    if (isMfaStepUpCancelled(error)) {
      return;
    }
    logger.error("Failed to import configs:", error);
  } finally {
    importing.value = false;
  }
};

watch(visible, (val) => {
  if (!val) {
    files.value = [];
  }
});
</script>
