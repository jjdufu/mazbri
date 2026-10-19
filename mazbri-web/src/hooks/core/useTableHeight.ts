import { computed, type Ref } from "vue";
interface TableHeightOptions {
  showTableHeader: Ref<boolean | undefined>;
  paginationHeight: Ref<number>;
  tableHeaderHeight: Ref<number>;
  paginationSpacing: Ref<number>;
}
class TableHeightCalculator {
  private static readonly TABLE_HEADER_SPACING = 12;
  constructor(private options: TableHeightOptions) {}
  calculate(): {
    height: string;
    minHeight?: string;
  } {
    const offset = this.calculateOffset();
    if (offset === 0) {
      return { height: "100%" };
    }
    return {
      height: `calc(100% - ${offset}px)`,
      minHeight: "200px",
    };
  }
  private calculateOffset(): number {
    if (!this.options.showTableHeader.value) {
      return 0;
    }
    const headerHeight = this.options.tableHeaderHeight.value;
    return headerHeight === 0
      ? 0
      : headerHeight + TableHeightCalculator.TABLE_HEADER_SPACING;
  }
}
export function useTableHeight(options: TableHeightOptions) {
  const containerHeight = computed(() => {
    const calculator = new TableHeightCalculator(options);
    return calculator.calculate();
  });
  return {
    containerHeight,
  };
}
