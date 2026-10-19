<template>
  <ImportDialog
    v-model="visible"
    :title="$t('certificate.presets.importDialog.title')"
    :columns="PRESET_IMPORT_COLUMNS"
    unique-key="service_name"
    :existing-data="existingData"
    :validator="validatePresetImportRow"
    :transformer="transformer"
    :on-import="handleImport"
    @success="emit('success')"
  />
</template>

<script setup lang="ts">
import { ElMessage } from "element-plus";
import type {
  CreatePresetRequest,
  PresetDnsName,
} from "@/types/api/certificate";
import ImportDialog from "@/components/common/ImportDialog.vue";
import { createPreset } from "@/api/certificate";
import { getLicenseDisabledReason } from "@/composables/useLicenseGate";
import {
  PRESET_IMPORT_COLUMNS,
  transformPresetImportRow,
  validatePresetImportRow,
} from "../usePresetImport";
interface Props {
  modelValue: boolean;
  existingPresets: PresetDnsName[];
  authorizationDisabled?: boolean;
  authorizationDisabledReason?: string;
}
const props = withDefaults(defineProps<Props>(), {
  authorizationDisabled: false,
  authorizationDisabledReason: getLicenseDisabledReason(),
});
const emit = defineEmits<{
  "update:modelValue": [value: boolean];
  success: [];
}>();
const visible = computed({
  get: () => props.modelValue,
  set: (value) => emit("update:modelValue", value),
});
const existingData = computed(
  () => props.existingPresets as unknown as Record<string, unknown>[],
);
const transformer = (row: Record<string, string>) =>
  transformPresetImportRow(row) as unknown as Record<string, unknown>;
const handleImport = async (items: Record<string, unknown>[]) => {
  if (props.authorizationDisabled) {
    ElMessage.warning(props.authorizationDisabledReason);
    throw new Error(props.authorizationDisabledReason);
  }
  for (const item of items as unknown as CreatePresetRequest[]) {
    await createPreset(item, {
      headers: {
        "x-mazbri-audit-action": "preset-import",
      },
    });
  }
};
</script>
