import { ElMessage, ElMessageBox } from "element-plus";
import type {
  CreatePresetRequest,
  PresetDnsName,
  UpdatePresetRequest,
} from "@/types/api/certificate";
import {
  createPreset,
  deletePreset,
  fetchPresetList,
  updatePreset,
} from "@/api/certificate";
import { useTableSelection } from "@/composables/useTableSelection";
import { exportResourceCsv } from "@/composables/useResourceCsvImportExport";
import { runWithLoading } from "@/composables/useAsyncState";
import { useLicenseGate } from "@/composables/useLicenseGate";
import { $t } from "@/locales";
import { logger } from "@/utils/sys";
import { PRESET_IMPORT_COLUMNS, sanitizePresetList } from "./usePresetImport";
type PresetExportRow = Record<string, unknown> & {
  service_name: string;
  dns_names: string[];
  description: string;
};
export function usePresetPage() {
  const loading = ref(false);
  const submitting = ref(false);
  const presets = ref<PresetDnsName[]>([]);
  const {
    licenseActive,
    authorizationDisabled,
    authorizationDisabledReason,
    loadLicenseStatus,
    ensureLicenseActive,
  } = useLicenseGate();
  const formVisible = ref(false);
  const importVisible = ref(false);
  const editingPreset = ref<PresetDnsName | null>(null);
  const {
    selectedRows,
    handleSelectionChange,
    handleDeleteCommand,
    clearSelection,
  } = useTableSelection<PresetDnsName>({
    deleteApi: (preset) => deletePreset(preset.id),
    getItemName: (preset) => preset.service_name,
    onDeleteSuccess: () => loadData(),
  });
  const loadData = async () => {
    try {
      await runWithLoading(loading, async () => {
        await loadLicenseStatus();
        try {
          const data = await fetchPresetList();
          presets.value = sanitizePresetList(data);
        } catch (error) {
          presets.value = [];
          if (licenseActive.value) {
            throw error;
          }
        }
      });
    } catch (error) {
      logger.error("Failed to load presets");
      licenseActive.value = false;
      presets.value = [];
    }
  };
  const openCreate = () => {
    if (!ensureLicenseActive()) return;
    editingPreset.value = null;
    formVisible.value = true;
  };
  const openImport = () => {
    if (!ensureLicenseActive()) return;
    importVisible.value = true;
  };
  const openEdit = (preset: PresetDnsName) => {
    if (!ensureLicenseActive()) return;
    editingPreset.value = preset;
    formVisible.value = true;
  };
  const submitPreset = async (payload: CreatePresetRequest) => {
    if (!ensureLicenseActive()) return;
    try {
      await runWithLoading(submitting, async () => {
        if (editingPreset.value) {
          const data: UpdatePresetRequest = {
            dns_names: payload.dns_names,
            description: payload.description,
          };
          await updatePreset(editingPreset.value.id, data);
        } else {
          await createPreset(payload);
        }
        formVisible.value = false;
        editingPreset.value = null;
        await loadData();
        ElMessage.success($t("common.feedback.saveSuccess"));
      });
    } catch (error) {
      logger.error("Failed to save preset");
    }
  };
  const deleteOne = async (preset: PresetDnsName) => {
    if (!ensureLicenseActive()) return;
    try {
      await ElMessageBox.confirm(
        $t("certificate.messages.deletePresetConfirm", {
          name: preset.service_name,
        }),
        $t("common.tips"),
        {
          confirmButtonText: $t("common.confirm"),
          cancelButtonText: $t("common.cancel"),
          type: "warning",
        },
      );
      await deletePreset(preset.id);
      await loadData();
      ElMessage.success($t("common.feedback.deleteSuccess"));
    } catch (error) {
      if (error !== "cancel") {
        logger.error("Failed to delete preset");
      }
    }
  };
  const exportData = (type: "all" | "selected") => {
    const dataToExport = type === "all" ? presets.value : selectedRows.value;
    if (dataToExport.length === 0) {
      ElMessage.warning(
        type === "all"
          ? $t("certificate.messages.noPresetToExport")
          : $t("certificate.messages.selectPresetToExport"),
      );
      return;
    }
    const date = new Date().toISOString().slice(0, 10);
    const exportRows: PresetExportRow[] = dataToExport.map((preset) => ({
      service_name: preset.service_name,
      dns_names: preset.dns_names,
      description: preset.description ?? "",
    }));
    exportResourceCsv(
      exportRows,
      PRESET_IMPORT_COLUMNS.map((column) => ({
        key: column.key as keyof PresetExportRow,
        label: column.label,
      })),
      `certificate-presets-${date}.csv`,
      {
        successMessage: $t("certificate.messages.exportPresetCountSuccess", {
          count: dataToExport.length,
        }),
      },
    );
  };
  const handleDeleteAction = async (type: "all" | "selected") => {
    if (!ensureLicenseActive()) return;
    await handleDeleteCommand(type, presets.value);
  };
  const handleImportSuccess = async () => {
    importVisible.value = false;
    clearSelection();
    await loadData();
  };
  return {
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
  };
}
