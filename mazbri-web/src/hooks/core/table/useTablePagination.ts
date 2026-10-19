import { computed, reactive, ref } from "vue";
import type { ComputedRef } from "vue";
import { useWindowSize } from "@vueuse/core";
import {
  applyPageSizeChange,
  resetTableSearchParams,
  shouldSkipCurrentPageChange,
} from "@/utils/table/tableUtils";
export interface TablePaginationControls<
  TParams extends Record<string, unknown>,
> {
  searchParams: TParams;
  pagination: Api.Common.PaginationParams;
  paginationMobile: ComputedRef<
    Api.Common.PaginationParams & {
      small: boolean;
    }
  >;
  resetPaginationState: () => void;
  applySizeChange: (newSize: number) => boolean;
  applyCurrentChange: (newCurrent: number) => boolean;
}
export function createTablePaginationControls<
  TParams extends Record<string, unknown>,
>(options: {
  apiParams: Partial<TParams>;
  pageKey: string;
  sizeKey: string;
}): TablePaginationControls<TParams> {
  const { apiParams, pageKey, sizeKey } = options;
  const searchParams = reactive(
    Object.assign(
      {
        [pageKey]: 1,
        [sizeKey]: 10,
      },
      apiParams || {},
    ) as TParams,
  );
  const pagination = reactive<Api.Common.PaginationParams>({
    current:
      ((searchParams as Record<string, unknown>)[pageKey] as number) || 1,
    size: ((searchParams as Record<string, unknown>)[sizeKey] as number) || 10,
    total: 0,
  });
  const { width } = useWindowSize();
  const paginationMobile = computed(() => ({
    ...pagination,
    small: width.value < 768,
  }));
  const isCurrentChanging = ref(false);
  const resetPaginationState = () => {
    const paramsRecord = searchParams as Record<string, unknown>;
    const resetPagination = resetTableSearchParams(
      paramsRecord,
      (apiParams || {}) as Record<string, unknown>,
      pageKey,
      sizeKey,
    );
    pagination.current = resetPagination.current;
    pagination.size = resetPagination.size;
  };
  const applySizeChange = (newSize: number) => {
    if (newSize <= 0) {
      return false;
    }
    const paramsRecord = searchParams as Record<string, unknown>;
    applyPageSizeChange(paramsRecord, pagination, pageKey, sizeKey, newSize);
    return true;
  };
  const applyCurrentChange = (newCurrent: number) => {
    if (newCurrent <= 0) {
      return false;
    }
    if (
      shouldSkipCurrentPageChange(
        isCurrentChanging.value,
        pagination.current,
        newCurrent,
      )
    ) {
      return false;
    }
    try {
      isCurrentChanging.value = true;
      const paramsRecord = searchParams as Record<string, unknown>;
      pagination.current = newCurrent;
      if (paramsRecord[pageKey] !== newCurrent) {
        paramsRecord[pageKey] = newCurrent;
      }
      return true;
    } finally {
      isCurrentChanging.value = false;
    }
  };
  return {
    searchParams: searchParams as TParams,
    pagination,
    paginationMobile,
    resetPaginationState,
    applySizeChange,
    applyCurrentChange,
  };
}
