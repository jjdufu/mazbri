import type { ConfigSummary } from "@/types/api/config-center";
import { ref, type Ref } from "vue";
import { ensureMfaBeforeOpen } from "@/utils/http/mfa-dialog-service";
import { logger } from "@/utils/sys";

export type ConfigCenterDialogSuccess =
  | "edit"
  | "rollback"
  | "import"
  | "export";

interface DialogOptions {
  currentConfig: Ref<ConfigSummary | null>;
  onSuccess: (kind: ConfigCenterDialogSuccess) => Promise<void> | void;
}

export function useConfigCenterDialogs(options: DialogOptions) {
  const editVisible = ref(false);
  const historyVisible = ref(false);
  const importVisible = ref(false);
  const exportVisible = ref(false);

  const ensurePlatformMfa = async (): Promise<boolean> => {
    try {
      return await ensureMfaBeforeOpen("platform");
    } catch (error: unknown) {
      logger.error("Failed to verify MFA before opening config dialog:", error);
      return false;
    }
  };

  const handleCreate = () => {
    options.currentConfig.value = null;
    editVisible.value = true;
  };

  const handleEdit = async (row: ConfigSummary) => {
    if (!(await ensurePlatformMfa())) return;
    options.currentConfig.value = row;
    editVisible.value = true;
  };

  const handleHistory = async (row: ConfigSummary) => {
    if (!(await ensurePlatformMfa())) return;
    options.currentConfig.value = row;
    historyVisible.value = true;
  };

  const handleImport = () => {
    importVisible.value = true;
  };

  const handleExport = async () => {
    if (!(await ensurePlatformMfa())) return;
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
