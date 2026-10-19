<template>
  <ControlPanelPageShell
    :title="$t('certificate.presets.page.title')"
    icon="ri:file-copy-line"
  >
    <PresetTableCard
      :loading="loading"
      :presets="presets"
      :selected-count="selectedRows.length"
      :authorization-disabled="authorizationDisabled"
      :authorization-disabled-reason="authorizationDisabledReason"
      @create="openCreate"
      @import="openImport"
      @export="exportData"
      @edit="openEdit"
      @delete="deleteOne"
      @delete-command="handleDeleteAction"
      @selection-change="handleSelectionChange"
    />

    <PresetFormDialog
      v-model="formVisible"
      :mode="editingPreset ? 'edit' : 'create'"
      :preset="editingPreset"
      :submitting="submitting"
      @submit="submitPreset"
    />

    <PresetImportDialog
      v-model="importVisible"
      :existing-presets="presets"
      :authorization-disabled="authorizationDisabled"
      :authorization-disabled-reason="authorizationDisabledReason"
      @success="handleImportSuccess"
    />
  </ControlPanelPageShell>
</template>

<script setup lang="ts">
import { $t } from "@/locales";
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import PresetFormDialog from "./modules/PresetFormDialog.vue";
import PresetImportDialog from "./modules/PresetImportDialog.vue";
import PresetTableCard from "./modules/PresetTableCard.vue";
import { usePresetPage } from "./usePresetPage";
defineOptions({ name: "CertificatePresets" });
const {
  loading,
  submitting,
  presets,
  authorizationDisabled,
  authorizationDisabledReason,
  selectedRows,
  formVisible,
  importVisible,
  editingPreset,
  loadData,
  openCreate,
  openImport,
  openEdit,
  submitPreset,
  deleteOne,
  exportData,
  handleSelectionChange,
  handleDeleteAction,
  handleImportSuccess,
} = usePresetPage();
onMounted(() => {
  loadData();
});
onActivated(() => {
  loadData();
});
</script>
