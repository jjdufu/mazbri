import { computed, reactive, ref } from "vue";
import { ElMessage, ElMessageBox } from "element-plus";
import { $t } from "@/locales";
import type { ConfigEntry, ConfigListQuery } from "@/types/api/config-center";
import type { ColumnOption } from "@/types/component";
import { logger } from "@/utils/sys";
import { formatDateTime as formatDate } from "@/utils/format";
export function useConfigCenterPage() {
  const columns = ref<ColumnOption[]>([
    { type: "selection", width: 55 },
    {
      prop: "service_name",
      label: $t("configCenter.columns.serviceName"),
      minWidth: 180,
      useSlot: true,
    },
    {
      prop: "updated_by",
      label: $t("configCenter.columns.updatedBy"),
      minWidth: 120,
      useSlot: true,
    },
    {
      prop: "updated_at",
      label: $t("configCenter.columns.updatedAt"),
      minWidth: 170,
      useSlot: true,
    },
    {
      prop: "operation",
      label: $t("configCenter.columns.operation"),
      width: 240,
      fixed: "right",
      useSlot: true,
    },
  ]);
  const loading = ref(false);
  const tableData = ref<ConfigEntry[]>([]);
  const currentConfig = ref<ConfigEntry | null>(null);
  const selectedRows = ref<ConfigEntry[]>([]);
  const searchForm = reactive<Partial<ConfigListQuery>>({
    q: undefined,
  });
  const pagination = reactive({
    current: 1,
    size: 20,
    total: 0,
  });
  const existingServiceNames = computed(() =>
    tableData.value.map((item) => item.service_name),
  );
  const handleSelectionChange = (selection: ConfigEntry[]) => {
    selectedRows.value = selection;
  };
  const clearSelection = () => {
    selectedRows.value = [];
  };
  const confirmDeleteMany = async (
    items: ConfigEntry[],
    deleteApi: (row: ConfigEntry) => Promise<void>,
    onDeleteSuccess: () => Promise<void> | void,
  ) => {
    if (items.length === 0) {
      ElMessage.warning($t("configCenter.messages.selectConfigToDelete"));
      return;
    }
    await ElMessageBox.confirm(
      $t("configCenter.messages.deleteManyConfirm", { count: items.length }),
      $t("configCenter.messages.deleteConfirmTitle"),
      {
        confirmButtonText: $t("configCenter.messages.confirmDelete"),
        cancelButtonText: $t("common.cancel"),
        type: "warning",
      },
    );
    await Promise.all(items.map((item) => deleteApi(item)));
    ElMessage.success(
      $t("configCenter.messages.deleteManySuccess", { count: items.length }),
    );
    clearSelection();
    await onDeleteSuccess();
  };
  const handleDeleteCommand = async (
    type: "all" | "selected",
    allData: ConfigEntry[],
    deleteApi: (row: ConfigEntry) => Promise<void>,
    onDeleteSuccess: () => Promise<void> | void,
  ) => {
    try {
      await confirmDeleteMany(
        type === "all" ? allData : selectedRows.value,
        deleteApi,
        onDeleteSuccess,
      );
    } catch (error: unknown) {
      if (error !== "cancel") {
        logger.error("Failed to batch delete config entries:", error);
        ElMessage.error($t("configCenter.messages.deleteManyFailed"));
      }
    }
  };
  const handleDeleteRow = async (
    row: ConfigEntry,
    deleteApi: (row: ConfigEntry) => Promise<void>,
    onDeleteSuccess: () => Promise<void> | void,
  ) => {
    try {
      await ElMessageBox.confirm(
        $t("configCenter.messages.deleteServiceConfigConfirm", {
          serviceName: row.service_name,
        }),
        $t("common.tips"),
        {
          confirmButtonText: $t("common.confirm"),
          cancelButtonText: $t("common.cancel"),
          type: "warning",
        },
      );
      await deleteApi(row);
      ElMessage.success($t("common.feedback.deleteSuccess"));
      await onDeleteSuccess();
    } catch (error: unknown) {
      if (error !== "cancel") {
        logger.error("Failed to delete config:", error);
      }
    }
  };
  return {
    columns,
    loading,
    tableData,
    currentConfig,
    selectedRows,
    searchForm,
    pagination,
    existingServiceNames,
    formatDate,
    handleSelectionChange,
    clearSelection,
    handleDeleteCommand,
    handleDeleteRow,
  };
}
