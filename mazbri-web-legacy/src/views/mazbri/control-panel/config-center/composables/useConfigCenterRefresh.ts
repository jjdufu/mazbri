import { fetchConfigList } from "@/api/config-center";
import type {
  ConfigListQuery,
  ConfigSummary,
} from "@/types/api/config-center";
import { logger } from "@/utils/sys";
import { watch, type Ref } from "vue";
import { useLatestRequest } from "@/composables/useLatestRequest";

interface RefreshState {
  loading: Ref<boolean>;
  tableData: Ref<ConfigSummary[]>;
  searchForm: Partial<ConfigListQuery>;
  pagination: {
    current: number;
    size: number;
    total: number;
  };
}

interface RefreshOptions {
  silent?: boolean;
  resetPage?: boolean;
  resetSearch?: boolean;
}

export function useConfigCenterRefresh(state: RefreshState) {
  const listRunner = useLatestRequest(async (ctx) => {
    const filteredSearch = Object.fromEntries(
      Object.entries(state.searchForm).filter(
        ([_, value]) => value !== undefined && value !== "",
      ),
    );
    const res = await fetchConfigList(
      {
        ...filteredSearch,
        page: state.pagination.current,
        page_size: state.pagination.size,
      },
      ctx.signal,
    );
    if (!ctx.isLatest()) return;
    state.tableData.value = res.configs;
    state.pagination.total = res.total;
  });

  watch(listRunner.loading, (val) => {
    state.loading.value = val;
  });

  const loadData = async (silent = false) => {
    try {
      if (silent) {
        await listRunner.runSilent();
      } else {
        await listRunner.run();
      }
    } catch (error) {
      logger.error("Failed to load config list:", error);
    }
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
