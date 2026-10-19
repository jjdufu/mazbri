import { computed, ref, type Ref } from "vue";
import { watchDebounced } from "@vueuse/core";
import type {
  ListFilter,
  ResourceFilterCatalog,
} from "@/types/list-query";
import {
  buildListQueryParams,
  type ListQueryEncodeMode,
} from "@/utils/list-query/serialize";
import {
  buildQPlaceholder,
  maxFilters,
  validateFilters,
} from "@/utils/list-query/catalog";
import { mergeByFieldOp } from "@/utils/list-query/merge-filters";

export interface UseListQueryOptions {
  catalog: ResourceFilterCatalog | Ref<ResourceFilterCatalog>;

  onChange: () => void;
  debounce?: number;
  maxWait?: number;
  encodeMode?: ListQueryEncodeMode;
  initialQ?: string;
  initialFilters?: ListFilter[];




  searchOnQChange?: boolean;
}





export function useListQuery(options: UseListQueryOptions) {
  const catalogRef = computed(() =>
    "value" in options.catalog ? options.catalog.value : options.catalog,
  );

  const q = ref(options.initialQ ?? "");
  const filters = ref<ListFilter[]>([...(options.initialFilters ?? [])]);
  const skipNextAutoSearch = ref(false);
  const lastSubmittedQ = ref((options.initialQ ?? "").trim());

  const hasActiveQ = computed(() => Boolean(q.value.trim()));
  const hasActiveFilters = computed(() => filters.value.length > 0);
  const hasActiveQuery = computed(
    () => hasActiveQ.value || hasActiveFilters.value,
  );

  const qPlaceholder = (t: (key: string) => string, fallback = "") =>
    buildQPlaceholder(catalogRef.value, t, fallback);

  const applyChange = () => {
    lastSubmittedQ.value = q.value.trim();
    options.onChange();
  };

  if (options.searchOnQChange !== false) {
    watchDebounced(
      () => q.value,
      (value, oldValue) => {
        if (value === oldValue) return;
        if (skipNextAutoSearch.value) {
          skipNextAutoSearch.value = false;
          return;
        }
        if (value.trim() === lastSubmittedQ.value) return;
        applyChange();
      },
      {
        debounce: options.debounce ?? 300,
        maxWait: options.maxWait ?? 600,
      },
    );
  }

  const handleSearch = () => {
    applyChange();
  };

  const handleClearSearch = () => {
    skipNextAutoSearch.value = true;
    q.value = "";
    applyChange();
  };

  const setFilters = (next: ListFilter[]) => {
    const err = validateFilters(catalogRef.value, next);
    if (err) {
      throw new Error(err);
    }
    filters.value = next;
    applyChange();
  };

  const addFilter = (filter: ListFilter) => {
    const next = mergeByFieldOp(filters.value, filter);
    if (next.length > maxFilters(catalogRef.value)) {
      throw new Error(
        `Too many filters (max ${maxFilters(catalogRef.value)})`,
      );
    }
    setFilters(next);
  };

  const removeFilterAt = (index: number) => {
    if (index < 0 || index >= filters.value.length) return;
    const next = filters.value.slice();
    next.splice(index, 1);
    filters.value = next;
    applyChange();
  };

  const clearFilters = () => {
    if (filters.value.length === 0) return;
    filters.value = [];
    applyChange();
  };

  const clearAll = () => {
    skipNextAutoSearch.value = true;
    q.value = "";
    filters.value = [];
    applyChange();
  };

  const applyQuickFilters = (quick: ListFilter[]) => {

    const map = new Map(
      filters.value.map((f) => [`${f.field}:${f.op}`, f] as const),
    );
    for (const f of quick) {
      map.set(`${f.field}:${f.op}`, f);
    }
    setFilters([...map.values()]);
  };


  const toRequestParams = (
    extra?: Record<string, unknown>,
  ): Record<string, unknown> =>
    buildListQueryParams({
      q: q.value,
      filters: filters.value,
      encodeMode: options.encodeMode ?? "both",
      extra,
    });

  return {
    catalog: catalogRef,
    q,
    filters,
    hasActiveQ,
    hasActiveFilters,
    hasActiveQuery,
    qPlaceholder,
    handleSearch,
    handleClearSearch,
    setFilters,
    addFilter,
    removeFilterAt,
    clearFilters,
    clearAll,
    applyQuickFilters,
    toRequestParams,
    applyChange,
  };
}

export type UseListQueryReturn = ReturnType<typeof useListQuery>;
