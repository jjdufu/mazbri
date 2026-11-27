import { computed, ref } from "vue";
import { $t } from "@/locales";
import type { ApiResponse, TableCache } from "@/utils/table/tableCache";
import type { TableError } from "@/utils/table/tableUtils";
import {
  buildTableRequestParams,
  extractTableData,
  syncPaginationKeys,
  updatePaginationFromResponse,
} from "@/utils/table/tableUtils";
type LoadingState = "idle" | "loading" | "success" | "error";
type AbortableApiFn<TParams extends Record<string, unknown>, TResponse> = (
  params: TParams,
  signal?: AbortSignal,
) => Promise<TResponse>;
export function createTableRequestController<
  TRecord,
  TParams extends Record<string, unknown>,
  TResponse,
>(config: {
  apiFn: (params: TParams) => Promise<TResponse>;
  searchParams: TParams;
  pagination: Api.Common.PaginationParams;
  pageKey: string;
  sizeKey: string;
  excludeParams: string[];
  cache: TableCache<TRecord> | null;
  enableCache: boolean;
  responseAdapter: (response: TResponse) => ApiResponse<TRecord>;
  dataTransformer?: (data: TRecord[]) => TRecord[];
  onSuccess?: (data: TRecord[], response: ApiResponse<TRecord>) => void;
  onCacheHit?: (data: TRecord[], response: ApiResponse<TRecord>) => void;
  handleError: (error: unknown, fallbackMessage: string) => TableError;
  onCacheMutate: () => void;
  logger: {
    log: (message: string) => void;
  };
}) {
  const {
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
    onCacheMutate,
    logger,
  } = config;
  const loadingState = ref<LoadingState>("idle");
  const loading = computed(() => loadingState.value === "loading");
  const error = ref<TableError | null>(null);
  const data = ref<TRecord[]>([]);
  let abortController: AbortController | null = null;
  const fetchData = async (
    params?: Partial<TParams>,
    useCache = enableCache,
  ): Promise<ApiResponse<TRecord>> => {
    if (abortController) {
      abortController.abort();
    }
    const currentController = new AbortController();
    abortController = currentController;
    loadingState.value = "loading";
    error.value = null;
    try {
      const requestParams = buildTableRequestParams<TParams>({
        searchParams,
        pageKey,
        sizeKey,
        current: pagination.current,
        size: pagination.size,
        params,
        excludeParams,
      });
      if (useCache && cache) {
        const cachedItem = cache.get(requestParams);
        if (cachedItem) {
          data.value = cachedItem.data;
          updatePaginationFromResponse(pagination, cachedItem.response);
          const paramsRecord = searchParams as Record<string, unknown>;
          syncPaginationKeys(
            paramsRecord,
            pageKey,
            sizeKey,
            pagination.current,
            pagination.size,
          );
          loadingState.value = "success";
          if (onCacheHit) {
            onCacheHit(cachedItem.data, cachedItem.response);
          }
          logger.log("Cache hit");
          return cachedItem.response;
        }
      }
      const response = await (apiFn as AbortableApiFn<TParams, TResponse>)(
        requestParams,
        currentController.signal,
      );
      if (currentController.signal.aborted) {
        throw new Error($t("httpMsg.requestCancelled"));
      }
      const standardResponse = responseAdapter(response);
      let tableData = extractTableData(standardResponse);
      if (dataTransformer) {
        tableData = dataTransformer(tableData);
      }
      data.value = tableData;
      updatePaginationFromResponse(pagination, standardResponse);
      const paramsRecord = searchParams as Record<string, unknown>;
      syncPaginationKeys(
        paramsRecord,
        pageKey,
        sizeKey,
        pagination.current,
        pagination.size,
      );
      if (useCache && cache) {
        cache.set(requestParams, tableData, standardResponse);
        onCacheMutate();
        logger.log("Data cached");
      }
      loadingState.value = "success";
      if (onSuccess) {
        onSuccess(tableData, standardResponse);
      }
      return standardResponse;
    } catch (err) {
      if (
        err instanceof Error &&
        err.message === $t("httpMsg.requestCancelled")
      ) {
        loadingState.value = "idle";
        return { records: [], total: 0, current: 1, size: 10 };
      }
      loadingState.value = "error";
      data.value = [];
      error.value = handleError(err, $t("common.table.fetchFailed"));
      throw error.value;
    } finally {
      if (abortController === currentController) {
        abortController = null;
      }
    }
  };
  const cancelActiveRequest = () => {
    if (abortController) {
      abortController.abort();
    }
  };
  return {
    data,
    error,
    loading,
    fetchData,
    cancelActiveRequest,
  };
}
