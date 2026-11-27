import { ref } from "vue";
import { ElMessage, ElMessageBox } from "element-plus";
import { $t } from "@/locales";
import { logger } from "@/utils/sys";
export interface UseTableSelectionOptions<T> {
  deleteApi: (item: T) => Promise<void>;
  getItemName: (item: T) => string;
  getItemId?: (item: T) => string;
  onDeleteSuccess?: () => void;
  allDeleteConfirmMsg?: (count: number) => string;
  selectedDeleteConfirmMsg?: (count: number) => string;
  actionLabel?: string;
}
export function useTableSelection<T>(options: UseTableSelectionOptions<T>) {
  const {
    deleteApi,
    getItemName,
    getItemId,
    onDeleteSuccess,
    allDeleteConfirmMsg,
    selectedDeleteConfirmMsg,
    actionLabel = $t("common.actions.delete"),
  } = options;
  const selectedRows = ref<T[]>([]);
  const handleSelectionChange = (selection: T[]) => {
    selectedRows.value = selection;
  };
  const clearSelection = () => {
    selectedRows.value = [];
  };
  const handleDeleteCommand = async (
    type: "all" | "selected",
    allData: T[],
  ) => {
    const dataToDelete = type === "all" ? allData : selectedRows.value;
    if (dataToDelete.length === 0) {
      ElMessage.warning(
        type === "all"
          ? $t("feedback.batch.noDataForAction", { action: actionLabel })
          : $t("feedback.batch.selectDataForAction", { action: actionLabel }),
      );
      return;
    }
    const count = dataToDelete.length;
    const defaultAllMsg = $t("feedback.batch.confirmAllAction", {
      action: actionLabel,
      count,
    });
    const defaultSelectedMsg = $t("feedback.batch.confirmSelectedAction", {
      action: actionLabel,
      count,
    });
    const confirmMsg =
      type === "all"
        ? (allDeleteConfirmMsg?.(count) ?? defaultAllMsg)
        : (selectedDeleteConfirmMsg?.(count) ?? defaultSelectedMsg);
    try {
      await ElMessageBox.confirm(
        confirmMsg,
        $t("feedback.batch.confirmActionTitle", { action: actionLabel }),
        {
          confirmButtonText: $t("feedback.batch.confirmActionButton", {
            action: actionLabel,
          }),
          cancelButtonText: $t("common.cancel"),
          type: "warning",
        },
      );
      const promises = dataToDelete.map((item) => deleteApi(item as T));
      await Promise.all(promises);
      ElMessage.success(
        $t("feedback.batch.actionSuccess", { action: actionLabel, count }),
      );
      clearSelection();
      onDeleteSuccess?.();
    } catch (error: unknown) {
      if (error !== "cancel") {
        logger.error("Failed to batch delete");
        ElMessage.error(
          $t("feedback.batch.actionFailed", { action: actionLabel }),
        );
      }
    }
  };
  return {
    selectedRows,
    handleSelectionChange,
    clearSelection,
    handleDeleteCommand,
  };
}
