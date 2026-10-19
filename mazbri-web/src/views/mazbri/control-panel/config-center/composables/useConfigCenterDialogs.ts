import type { ConfigEntry } from "@/types/api/config-center";
import { ref } from "vue";
import type { Ref } from "vue";
export type ConfigCenterDialogSuccess =
  | "edit"
  | "rollback"
  | "import"
  | "export";
interface DialogOptions {
  currentConfig: Ref<ConfigEntry | null>;
  onSuccess: (kind: ConfigCenterDialogSuccess) => Promise<void> | void;
}
export function useConfigCenterDialogs(options: DialogOptions) {
  const editVisible = ref(false);
  const historyVisible = ref(false);
  const importVisible = ref(false);
  const exportVisible = ref(false);
  const handleCreate = () => {
    options.currentConfig.value = null;
    editVisible.value = true;
  };
  const handleEdit = (row: ConfigEntry) => {
    options.currentConfig.value = row;
    editVisible.value = true;
  };
  const handleHistory = (row: ConfigEntry) => {
    options.currentConfig.value = row;
    historyVisible.value = true;
  };
  const handleImport = () => {
    importVisible.value = true;
  };
  const handleExport = () => {
    exportVisible.value = true;
  };
  const handleDialogSuccess = async (kind: ConfigCenterDialogSuccess) => {
    if (kind === "edit") editVisible.value = false;
    if (kind === "rollback") historyVisible.value = false;
    if (kind === "import") importVisible.value = false;
    if (kind === "export") exportVisible.value = false;
    await options.onSuccess(kind);
  };
  return {
    editVisible,
    historyVisible,
    importVisible,
    exportVisible,
    handleCreate,
    handleEdit,
    handleHistory,
    handleImport,
    handleExport,
    handleDialogSuccess,
  };
}
