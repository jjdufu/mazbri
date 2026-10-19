import { fetchConfigList } from "@/api/config-center";
import type {
  ConfigEntry,
  ConfigListQuery,
} from "@/types/api/config-center";
import { logger } from "@/utils/sys";
import type { Ref } from "vue";
import { runWithLoading } from "@/composables/useAsyncState";
interface RefreshState {
  loading: Ref<boolean>;
  tableData: Ref<ConfigEntry[]>;
  selectedRows: Ref<ConfigEntry[]>;
  searchForm: Partial<ConfigListQuery>;
  pagination: {
    current: number;
    size: number;
    total: number;
  };
  clearSelection: () => void;
}
interface RefreshOptions {
  silent?: boolean;
  resetPage?: boolean;
  resetSearch?: boolean;
  reloadServices?: boolean;
  clearSelection?: boolean;
}
export function useConfigCenterRefresh(state: RefreshState) {
  const loadDataCore = async () => {
    const previousSelection = new Set(
      state.selectedRows.value.map((row) => row.service_name),
    );
    const filteredSearch = Object.fromEntries(
      Object.entries(state.searchForm).filter(
        ([_, value]) => value !== undefined && value !== "",
      ),
    );
    const res = await fetchConfigList({
      ...filteredSearch,
      page: state.pagination.current,
      page_size: state.pagination.size,
    });
    state.tableData.value = res.configs;
    state.pagination.total = res.total;
    state.selectedRows.value = state.tableData.value.filter((row) =>
      previousSelection.has(row.service_name),
    );
  };
  const loadData = async (silent = false) => {
    if (silent) {
      try {
        await loadDataCore();
      } catch (error) {
        logger.error("Failed to load config list:", error);
      }
      return;
    }
    await runWithLoading(state.loading, loadDataCore).catch((error) => {
      logger.error("Failed to load config list:", error);
    });
  };
  const refresh = async (options: RefreshOptions = {}) => {
    if (options.resetSearch) {
      state.searchForm.q = undefined;
      state.searchForm.filters = undefined;
    }
    if (options.resetPage) {
      state.pagination.current = 1;
    }
    await loadData(options.silent);
    if (options.clearSelection) {
      state.clearSelection();
    }
  };
  const handleSearch = async () => {
    state.pagination.current = 1;
    await loadData();
  };
  const handleSizeChange = async (size: number) => {
    state.pagination.size = size;
    state.pagination.current = 1;
    await loadData();
  };
  const handleCurrentChange = async (current: number) => {
    state.pagination.current = current;
    await loadData();
  };
  const initialize = async () => {
    await loadData();
  };
  return {
    loadData,
    refresh,
    handleSearch,
    handleSizeChange,
    handleCurrentChange,
    initialize,
  };
}
