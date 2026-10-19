import { computed, ref, onMounted, onUnmounted, type Ref } from "vue";
import { storeToRefs } from "pinia";
import { useI18n } from "vue-i18n";
import { TableSizeEnum } from "@/enums/formEnum";
import { useTableStore } from "@/store/modules/table";
import type { ColumnOption } from "@/types/component";
export interface TableSizeOption {
  value: TableSizeEnum;
  label: string;
}
export interface UseTableHeaderProps {
  showZebra?: boolean;
  showBorder?: boolean;
  showHeaderBackground?: boolean;
  fullClass?: string;
  layout?: string;
  loading?: boolean;
  showSearchBar?: boolean;
}
export interface UseTableHeaderReturn {
  t: ReturnType<typeof useI18n>["t"];
  tableSizeOptions: TableSizeOption[];
  tableSize: Ref<TableSizeEnum>;
  isZebra: Ref<boolean>;
  isBorder: Ref<boolean>;
  isHeaderBackground: Ref<boolean>;
  isManualRefresh: Ref<boolean>;
  isFullScreen: Ref<boolean>;
  shouldShow: (componentName: string) => boolean;
  getColumnVisibility: (col: ColumnOption) => boolean;
  updateColumnVisibility: (
    col: ColumnOption,
    value: boolean | string | number,
  ) => void;
  checkColumnMove: (event: { related: HTMLElement }) => boolean;
  search: () => void;
  refresh: () => void;
  handleTableSizeChange: (command: TableSizeEnum) => void;
  toggleFullScreen: () => void;
}
export const useTableHeader = (
  props: UseTableHeaderProps,
  columns: {
    value: ColumnOption[];
  },
  emit: {
    (e: "refresh"): void;
    (e: "search"): void;
    (e: "update:showSearchBar", value: boolean): void;
  },
): UseTableHeaderReturn => {
  const { t } = useI18n();
  const tableStore = useTableStore();
  const { tableSize, isZebra, isBorder, isHeaderBackground } =
    storeToRefs(tableStore);
  const isManualRefresh = ref(false);
  const isFullScreen = ref(false);
  const originalOverflow = ref("");
  const tableSizeOptions: TableSizeOption[] = [
    { value: TableSizeEnum.SMALL, label: t("table.sizeOptions.small") },
    { value: TableSizeEnum.DEFAULT, label: t("table.sizeOptions.default") },
    { value: TableSizeEnum.LARGE, label: t("table.sizeOptions.large") },
  ];
  const layoutItems = computed(() => {
    return (props.layout ?? "").split(",").map((item) => item.trim());
  });
  const shouldShow = (componentName: string): boolean => {
    return layoutItems.value.includes(componentName);
  };
  const getColumnVisibility = (col: ColumnOption): boolean => {
    if (col.visible !== undefined) {
      return col.visible;
    }
    return col.checked ?? true;
  };
  const updateColumnVisibility = (
    col: ColumnOption,
    value: boolean | string | number,
  ): void => {
    const boolValue = !!value;
    col.checked = boolValue;
    col.visible = boolValue;
  };
  const checkColumnMove = (event: { related: HTMLElement }): boolean => {
    const toElement = event.related;
    if (toElement && toElement.classList.contains("fixed-column")) {
      return false;
    }
    return true;
  };
  const search = (): void => {
    emit("update:showSearchBar", !props.showSearchBar);
    emit("search");
  };
  const refresh = (): void => {
    isManualRefresh.value = true;
    emit("refresh");
  };
  const handleTableSizeChange = (command: TableSizeEnum): void => {
    tableStore.setTableSize(command);
  };
  const toggleFullScreen = (): void => {
    const el = document.querySelector(`.${props.fullClass}`);
    if (!el) return;
    isFullScreen.value = !isFullScreen.value;
    if (isFullScreen.value) {
      originalOverflow.value = document.body.style.overflow;
      document.body.style.overflow = "hidden";
      el.classList.add("el-full-screen");
      tableStore.setIsFullScreen(true);
    } else {
      document.body.style.overflow = originalOverflow.value;
      el.classList.remove("el-full-screen");
      tableStore.setIsFullScreen(false);
    }
  };
  const handleEscapeKey = (e: KeyboardEvent): void => {
    if (e.key === "Escape" && isFullScreen.value) {
      toggleFullScreen();
    }
  };
  onMounted(() => {
    document.addEventListener("keydown", handleEscapeKey);
  });
  onUnmounted(() => {
    document.removeEventListener("keydown", handleEscapeKey);
    if (isFullScreen.value) {
      document.body.style.overflow = originalOverflow.value;
      const el = document.querySelector(`.${props.fullClass}`);
      if (el) {
        el.classList.remove("el-full-screen");
      }
    }
  });
  return {
    t,
    tableSizeOptions,
    tableSize,
    isZebra,
    isBorder,
    isHeaderBackground,
    isManualRefresh,
    isFullScreen,
    shouldShow,
    getColumnVisibility,
    updateColumnVisibility,
    checkColumnMove,
    search,
    refresh,
    handleTableSizeChange,
    toggleFullScreen,
  };
};
