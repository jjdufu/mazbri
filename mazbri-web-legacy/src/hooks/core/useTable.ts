import { ref, computed, onMounted, onUnmounted, nextTick, readonly } from "vue";
import type { Ref, ComputedRef } from "vue";
import { useTableColumns } from "./useTableColumns";
import { createTablePaginationControls } from "./table/useTablePagination";
import { createTableRequestController } from "./table/useTableRequest";
import type { ColumnOption } from "@/types/component";
import {
  TableCache,
  CacheInvalidationStrategy,
  type ApiResponse,
} from "../../utils/table/tableCache";
import {
  type TableError,
  defaultResponseAdapter,
  createSmartDebounce,
  createErrorHandler,
} from "../../utils/table/tableUtils";
import { tableConfig } from "../../utils/table/tableConfig";
import { logger as sysLogger } from "@/utils/sys";
type InferApiParams<T> = T extends (params: infer P) => any ? P : never;
type InferApiResponse<T> = T extends (params: any) => Promise<infer R>
  ? R
  : never;
type InferRecordType<T> =
  T extends Api.Common.PaginatedResponse<infer U>
    ? U
    : T extends {
          items: (infer U)[];
        }
      ? U
      : T extends {
            tenants: (infer U)[];
          }
        ? U
        : T extends {
              products: (infer U)[];
            }
          ? U
          : T extends {
                authorizations: (infer U)[];
              }
            ? U
            : T extends {
                  history: (infer U)[];
                }
              ? U
              : T extends {
                    configs: (infer U)[];
                  }
                ? U
                : T extends {
                      nodes: (infer U)[];
                    }
                  ? U
                  : T extends {
                        transactions: (infer U)[];
                      }
                    ? U
                    : never;
type TableAsyncFn<TArgs extends unknown[] = unknown[], TResult = unknown> = (
  ...args: TArgs
) => Promise<TResult>;
export interface UseTableConfig<
  TApiFn extends (params: any) => Promise<any> = (params: any) => Promise<any>,
  TRecord = InferRecordType<InferApiResponse<TApiFn>>,
  TParams = InferApiParams<TApiFn>,
  TResponse = InferApiResponse<TApiFn>,
> {
  core: {
    apiFn: TApiFn;
    apiParams?: Partial<TParams>;
    excludeParams?: string[];
    immediate?: boolean;
    columnsFactory?: () => ColumnOption<TRecord>[];
    paginationKey?: {
      current?: string;
      size?: string;
    };
  };
  transform?: {
    dataTransformer?: (data: TRecord[]) => TRecord[];
    responseAdapter?: (response: TResponse) => ApiResponse<TRecord>;
  };
  performance?: {
    enableCache?: boolean;
    cacheTime?: number;
    debounceTime?: number;
    maxCacheSize?: number;
  };
  hooks?: {
    onSuccess?: (data: TRecord[], response: ApiResponse<TRecord>) => void;
    onError?: (error: TableError) => void;
    onCacheHit?: (data: TRecord[], response: ApiResponse<TRecord>) => void;
    onLoading?: (loading: boolean) => void;
    resetFormCallback?: () => void;
  };
  debug?: {
    enableLog?: boolean;
    logLevel?: "info" | "warn" | "error";
  };
}
interface UseTableBaseResult<TRecord, TParams extends Record<string, unknown>> {
  data: Ref<any[]>;
  loading: Readonly<Ref<boolean>>;
  error: Readonly<Ref<TableError | null>>;
  isEmpty: ComputedRef<boolean>;
  hasData: ComputedRef<boolean>;
  pagination: Readonly<Api.Common.PaginationParams>;
  paginationMobile: ComputedRef<
    Api.Common.PaginationParams & {
      small: boolean;
    }
  >;
  handleSizeChange: (newSize: number) => Promise<void>;
  handleCurrentChange: (newCurrent: number) => Promise<void>;
  searchParams: TParams;
  resetSearchParams: () => Promise<void>;
  fetchData: (
    params?: Partial<TParams>,
  ) => Promise<ApiResponse<TRecord> | void>;
  getData: (params?: Partial<TParams>) => Promise<ApiResponse<TRecord> | void>;
  getDataDebounced: ((
    params?: Partial<TParams>,
  ) => Promise<ApiResponse<TRecord> | void>) & {
    cancel: () => void;
  };
  clearData: () => void;
  refreshData: () => Promise<void>;
  refreshSoft: () => Promise<void>;
  refreshCreate: () => Promise<void>;
  refreshUpdate: () => Promise<void>;
  refreshRemove: () => Promise<void>;
  cacheInfo: ComputedRef<{
    total: number;
    size: string;
    hitRate: string;
  }>;
  clearCache: (strategy: CacheInvalidationStrategy, context?: string) => void;
  clearExpiredCache: () => number;
  cancelRequest: () => void;
}
type UseTableColumnResult<TRecord> = Partial<{
  columns: ReturnType<typeof useTableColumns<TRecord>>["columns"];
  columnChecks: ReturnType<typeof useTableColumns<TRecord>>["columnChecks"];
  addColumn: ReturnType<typeof useTableColumns<TRecord>>["addColumn"];
  removeColumn: ReturnType<typeof useTableColumns<TRecord>>["removeColumn"];
  toggleColumn: ReturnType<typeof useTableColumns<TRecord>>["toggleColumn"];
  updateColumn: ReturnType<typeof useTableColumns<TRecord>>["updateColumn"];
  batchUpdateColumns: ReturnType<
    typeof useTableColumns<TRecord>
  >["batchUpdateColumns"];
  reorderColumns: ReturnType<typeof useTableColumns<TRecord>>["reorderColumns"];
  getColumnConfig: ReturnType<
    typeof useTableColumns<TRecord>
  >["getColumnConfig"];
  getAllColumns: ReturnType<typeof useTableColumns<TRecord>>["getAllColumns"];
  resetColumns: ReturnType<typeof useTableColumns<TRecord>>["resetColumns"];
}>;
type UseTableResult<
  TRecord,
  TParams extends Record<string, unknown>,
> = UseTableBaseResult<TRecord, TParams> & UseTableColumnResult<TRecord>;
export function useTable<TApiFn extends (params: any) => Promise<any>>(
  config: UseTableConfig<TApiFn>,
): UseTableResult<
  InferRecordType<InferApiResponse<TApiFn>>,
  InferApiParams<TApiFn>
> {
  return useTableImpl(config);
}
function useTableImpl<TApiFn extends (params: any) => Promise<any>>(
  config: UseTableConfig<TApiFn>,
): UseTableResult<
  InferRecordType<InferApiResponse<TApiFn>>,
  InferApiParams<TApiFn>
> {
  type TRecord = InferRecordType<InferApiResponse<TApiFn>>;
  type TParams = InferApiParams<TApiFn>;
  type TResponse = InferApiResponse<TApiFn>;
  const {
    core: {
      apiFn,
      apiParams = {} as Partial<TParams>,
      excludeParams = [],
      immediate = true,
      columnsFactory,
      paginationKey,
    },
    transform: {
      dataTransformer,
      responseAdapter = defaultResponseAdapter,
    } = {},
    performance: {
      enableCache = false,
      cacheTime = 5 * 60 * 1000,
      debounceTime = 300,
      maxCacheSize = 50,
    } = {},
    hooks: { onSuccess, onError, onCacheHit, resetFormCallback } = {},
    debug: { enableLog = false } = {},
  } = config;
  const pageKey = paginationKey?.current || tableConfig.paginationKey.current;
  const sizeKey = paginationKey?.size || tableConfig.paginationKey.size;
  const cacheUpdateTrigger = ref(0);
  const logger = {
    log: (message: string, ..._args: unknown[]) => {
      void message;
    },
    warn: (message: string, ..._args: unknown[]) => {
      if (enableLog) {
        sysLogger.warn(`[useTable] ${message}`);
      }
    },
    error: (message: string, ..._args: unknown[]) => {
      if (enableLog) {
        sysLogger.error(`[useTable] ${message}`);
      }
    },
  };
  const cache = enableCache
    ? new TableCache<TRecord>(cacheTime, maxCacheSize, enableLog)
    : null;
  let cacheCleanupTimer: NodeJS.Timeout | null = null;
  const {
    searchParams,
    pagination,
    paginationMobile: mobilePagination,
    resetPaginationState,
    applySizeChange,
    applyCurrentChange,
  } = createTablePaginationControls<TParams>({
    apiParams,
    pageKey,
    sizeKey,
  });
  const columnConfig = columnsFactory
    ? useTableColumns<TRecord>(columnsFactory)
    : null;
  const columns = columnConfig?.columns;
  const columnChecks = columnConfig?.columnChecks;
  const hasData = computed(() => data.value.length > 0);
  const cacheInfo = computed(() => {
    void cacheUpdateTrigger.value;
    if (!cache) return { total: 0, size: "0KB", hitRate: "0 avg hits" };
    return cache.getStats();
  });
  const handleError = createErrorHandler(onError, enableLog);
  const clearCache = (
    strategy: CacheInvalidationStrategy,
    context?: string,
  ): void => {
    if (!cache) return;
    let clearedCount = 0;
    switch (strategy) {
      case CacheInvalidationStrategy.CLEAR_ALL:
        cache.clear();
        logger.log(`Cleared all cache - ${context || ""}`);
        break;
      case CacheInvalidationStrategy.CLEAR_CURRENT:
        clearedCount = cache.clearCurrentSearch(searchParams);
        logger.log(
          `Cleared current search cache (${clearedCount}) - ${context || ""}`,
        );
        break;
      case CacheInvalidationStrategy.CLEAR_PAGINATION:
        clearedCount = cache.clearPagination();
        logger.log(
          `Cleared pagination cache (${clearedCount}) - ${context || ""}`,
        );
        break;
      case CacheInvalidationStrategy.KEEP_ALL:
      default:
        logger.log(`Kept cache unchanged - ${context || ""}`);
        break;
    }
    cacheUpdateTrigger.value++;
  };
  const { data, error, loading, fetchData, cancelActiveRequest } =
    createTableRequestController<TRecord, TParams, TResponse>({
      apiFn,
      searchParams,
      pagination,
      pageKey,
      sizeKey,
      excludeParams,
      cache,
      enableCache,
      responseAdapter,
      dataTransformer,
      onSuccess,
      onCacheHit,
      handleError,
      onCacheMutate: () => {
        cacheUpdateTrigger.value++;
      },
      logger,
    });
  const getData = async (
    params?: Partial<TParams>,
  ): Promise<ApiResponse<TRecord> | void> => {
    try {
      return await fetchData(params);
    } catch {
      return Promise.resolve();
    }
  };
  const getDataByPage = async (
    params?: Partial<TParams>,
  ): Promise<ApiResponse<TRecord> | void> => {
    pagination.current = 1;
    (searchParams as Record<string, unknown>)[pageKey] = 1;
    clearCache(CacheInvalidationStrategy.CLEAR_CURRENT, "search data");
    try {
      return await fetchData(params, false);
    } catch {
      return Promise.resolve();
    }
  };
  const debouncedGetDataByPage = createSmartDebounce(
    getDataByPage,
    debounceTime,
  );
  const resetSearchParams = async (): Promise<void> => {
    debouncedGetDataByPage.cancel();
    resetPaginationState();
    error.value = null;
    clearCache(CacheInvalidationStrategy.CLEAR_ALL, "reset search");
    await getData();
    if (resetFormCallback) {
      await nextTick();
      resetFormCallback();
    }
  };
  const handleSizeChange = async (newSize: number): Promise<void> => {
    debouncedGetDataByPage.cancel();
    if (!applySizeChange(newSize)) return;
    clearCache(CacheInvalidationStrategy.CLEAR_CURRENT, "page size change");
    await getData();
  };
  const handleCurrentChange = async (newCurrent: number): Promise<void> => {
    if (!applyCurrentChange(newCurrent)) {
      logger.log("Page number did not change; skipping request");
      return;
    }
    await getData();
  };
  const refreshCreate = async (): Promise<void> => {
    debouncedGetDataByPage.cancel();
    pagination.current = 1;
    (searchParams as Record<string, unknown>)[pageKey] = 1;
    clearCache(CacheInvalidationStrategy.CLEAR_PAGINATION, "create data");
    await getData();
  };
  const refreshUpdate = async (): Promise<void> => {
    clearCache(CacheInvalidationStrategy.CLEAR_CURRENT, "update data");
    await getData();
  };
  const refreshRemove = async (): Promise<void> => {
    const { current } = pagination;
    clearCache(CacheInvalidationStrategy.CLEAR_CURRENT, "delete data");
    await getData();
    if (data.value.length === 0 && current > 1) {
      pagination.current = current - 1;
      (searchParams as Record<string, unknown>)[pageKey] = current - 1;
      await getData();
    }
  };
  const refreshData = async (): Promise<void> => {
    debouncedGetDataByPage.cancel();
    clearCache(CacheInvalidationStrategy.CLEAR_ALL, "manual refresh");
    await getData();
  };
  const refreshSoft = async (): Promise<void> => {
    clearCache(CacheInvalidationStrategy.CLEAR_CURRENT, "soft refresh");
    await getData();
  };
  const cancelRequest = (): void => {
    cancelActiveRequest();
    debouncedGetDataByPage.cancel();
  };
  const clearData = (): void => {
    data.value = [];
    error.value = null;
    clearCache(CacheInvalidationStrategy.CLEAR_ALL, "clear data");
  };
  const clearExpiredCache = (): number => {
    if (!cache) return 0;
    const cleanedCount = cache.cleanupExpired();
    if (cleanedCount > 0) {
      cacheUpdateTrigger.value++;
    }
    return cleanedCount;
  };
  if (enableCache && cache) {
    cacheCleanupTimer = setInterval(() => {
      const cleanedCount = cache.cleanupExpired();
      if (cleanedCount > 0) {
        logger.log(`Auto-cleaned ${cleanedCount} expired cache entries`);
        cacheUpdateTrigger.value++;
      }
    }, cacheTime / 2);
  }
  if (immediate) {
    onMounted(async () => {
      await getData();
    });
  }
  onUnmounted(() => {
    cancelRequest();
    if (cache) {
      cache.clear();
    }
    if (cacheCleanupTimer) {
      clearInterval(cacheCleanupTimer);
    }
  });
  const columnBindings = columnConfig
    ? {
        columns,
        columnChecks,
        addColumn: columnConfig.addColumn,
        removeColumn: columnConfig.removeColumn,
        toggleColumn: columnConfig.toggleColumn,
        updateColumn: columnConfig.updateColumn,
        batchUpdateColumns: columnConfig.batchUpdateColumns,
        reorderColumns: columnConfig.reorderColumns,
        getColumnConfig: columnConfig.getColumnConfig,
        getAllColumns: columnConfig.getAllColumns,
        resetColumns: columnConfig.resetColumns,
      }
    : {};
  return {
    data,
    loading: readonly(loading),
    error: readonly(error),
    isEmpty: computed(() => data.value.length === 0),
    hasData,
    pagination: readonly(pagination),
    paginationMobile: mobilePagination,
    handleSizeChange,
    handleCurrentChange,
    searchParams,
    resetSearchParams,
    fetchData: getData,
    getData: getDataByPage,
    getDataDebounced: debouncedGetDataByPage,
    clearData,
    refreshData,
    refreshSoft,
    refreshCreate,
    refreshUpdate,
    refreshRemove,
    cacheInfo,
    clearCache,
    clearExpiredCache,
    cancelRequest,
    ...columnBindings,
  } as UseTableResult<TRecord, TParams>;
}
export { CacheInvalidationStrategy } from "../../utils/table/tableCache";
export type { ApiResponse, CacheItem } from "../../utils/table/tableCache";
export type {
  BaseRequestParams,
  TableError,
} from "../../utils/table/tableUtils";
