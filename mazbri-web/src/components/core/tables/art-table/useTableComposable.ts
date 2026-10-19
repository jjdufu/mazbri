import {
  ref,
  computed,
  nextTick,
  watchEffect,
  type Ref,
  type ComputedRef,
} from "vue";
import type { ElTable } from "element-plus";
import { useDebounceFn, useResizeObserver, useWindowSize } from "@vueuse/core";
import { useTableHeight } from "@/hooks/core/useTableHeight";
import { useCommon } from "@/hooks/core/useCommon";
import type { ColumnOption, PaginationConfig, ArtTableProps } from "./types";
export interface UseTableComposableReturn {
  width: Ref<number>;
  elTableRef: Ref<InstanceType<typeof ElTable> | null>;
  paginationRef: Ref<HTMLElement | undefined>;
  tableHeaderRef: Ref<HTMLElement | undefined>;
  paginationHeight: Ref<number>;
  tableHeaderHeight: Ref<number>;
  containerHeight: ComputedRef<Record<string, string>>;
  cleanColumnProps: (col: ColumnOption) => Record<string, unknown>;
  handleSizeChange: (val: number) => void;
  handleCurrentChange: (val: number) => void;
  scrollToTop: () => void;
  getGlobalIndex: (index: number) => number;
}
export const useTableComposable = (
  props: ArtTableProps,
  emit: {
    (e: "pagination:size-change", val: number): void;
    (e: "pagination:current-change", val: number): void;
  },
): UseTableComposableReturn => {
  const { width } = useWindowSize();
  const elTableRef = ref<InstanceType<typeof ElTable> | null>(null);
  const paginationRef = ref<HTMLElement>();
  const tableHeaderRef = ref<HTMLElement>();
  const paginationHeight = ref(0);
  const tableHeaderHeight = ref(0);
  const { scrollToTop: scrollPageToTop } = useCommon();
  const debouncedPaginationResize = useDebounceFn(
    (entries: ResizeObserverEntry[]) => {
      const entry = entries[0];
      if (entry) {
        requestAnimationFrame(() => {
          paginationHeight.value = entry.contentRect.height;
        });
      }
    },
    100,
  );
  const debouncedTableHeaderResize = useDebounceFn(
    (entries: ResizeObserverEntry[]) => {
      const entry = entries[0];
      if (entry) {
        requestAnimationFrame(() => {
          tableHeaderHeight.value = entry.contentRect.height;
        });
      }
    },
    100,
  );
  useResizeObserver(paginationRef, (entries) => {
    debouncedPaginationResize(Array.from(entries));
  });
  useResizeObserver(tableHeaderRef, (entries) => {
    debouncedTableHeaderResize(Array.from(entries));
  });
  const PAGINATION_SPACING = computed(() => (props.showTableHeader ? 6 : 15));
  const { containerHeight } = useTableHeight({
    showTableHeader: computed(() => props.showTableHeader ?? false),
    paginationHeight,
    tableHeaderHeight,
    paginationSpacing: PAGINATION_SPACING,
  });
  const cleanColumnProps = (col: ColumnOption): Record<string, unknown> => {
    const columnProps: Record<string, unknown> = { ...col };
    delete columnProps.useHeaderSlot;
    delete columnProps.headerSlotName;
    delete columnProps.useSlot;
    delete columnProps.slotName;

    const colType = columnProps.type;
    const prop = String(columnProps.prop ?? "");
    const isOperations =
      prop === "operations" ||
      col.slotName === "operations" ||
      col.prop === "operations";

    if (isOperations && columnProps.fixed !== "left") {
      columnProps.fixed = "right";
    }
    if (
      columnProps.showOverflowTooltip === undefined &&
      colType !== "selection" &&
      colType !== "expand" &&
      colType !== "index" &&
      colType !== "globalIndex" &&
      !isOperations
    ) {
      columnProps.showOverflowTooltip = true;
    }
    return columnProps;
  };
  const handleSizeChange = (val: number): void => {
    emit("pagination:size-change", val);
  };
  const handleCurrentChange = (val: number): void => {
    emit("pagination:current-change", val);
    scrollToTop();
  };
  const scrollToTop = (): void => {
    nextTick(() => {
      elTableRef.value?.setScrollTop(0);
      scrollPageToTop();
    });
  };
  const getGlobalIndex = (index: number): number => {
    if (!props.pagination) return index + 1;
    const { current, size } = props.pagination;
    return (current - 1) * size + index + 1;
  };
  const findTableHeader = (): void => {
    if (!props.showTableHeader) {
      tableHeaderRef.value = undefined;
      return;
    }
    const tableHeader = document.getElementById("art-table-header");
    tableHeaderRef.value = tableHeader ?? undefined;
  };
  watchEffect(
    () => {
      void props.data?.length;
      const shouldShow = props.showTableHeader;
      if (shouldShow) {
        nextTick(() => {
          findTableHeader();
        });
      } else {
        tableHeaderRef.value = undefined;
      }
    },
    { flush: "post" },
  );
  return {
    width,
    elTableRef,
    paginationRef,
    tableHeaderRef,
    paginationHeight,
    tableHeaderHeight,
    containerHeight,
    cleanColumnProps,
    handleSizeChange,
    handleCurrentChange,
    scrollToTop,
    getGlobalIndex,
  };
};
