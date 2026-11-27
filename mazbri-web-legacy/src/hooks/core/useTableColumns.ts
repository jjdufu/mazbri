import { ref, computed, watch } from "vue";
import { $t } from "@/locales";
import type { ColumnOption } from "@/types/component";
const SPECIAL_COLUMNS: Record<
  string,
  {
    prop: string;
    label: string;
  }
> = {
  selection: { prop: "__selection__", label: $t("table.column.selection") },
  expand: { prop: "__expand__", label: $t("table.column.expand") },
  index: { prop: "__index__", label: $t("table.column.index") },
};
export const getColumnKey = <T>(col: ColumnOption<T>) =>
  SPECIAL_COLUMNS[col.type as keyof typeof SPECIAL_COLUMNS]?.prop ??
  (col.prop as string);
export const getColumnVisibility = <T>(col: ColumnOption<T>): boolean => {
  if (col.visible !== undefined) {
    return col.visible;
  }
  return col.checked ?? true;
};
export const getColumnChecks = <T>(columns: ColumnOption<T>[]) =>
  columns.map((col) => {
    const special = col.type && SPECIAL_COLUMNS[col.type];
    const visibility = getColumnVisibility(col);
    if (special) {
      return {
        ...col,
        prop: special.prop,
        label: special.label,
        checked: true,
        visible: true,
      };
    }
    return { ...col, checked: visibility, visible: visibility };
  });
export interface DynamicColumnConfig<T = any> {
  addColumn: (
    column: ColumnOption<T> | ColumnOption<T>[],
    index?: number,
  ) => void;
  removeColumn: (prop: string | string[]) => void;
  toggleColumn: (prop: string | string[], visible?: boolean) => void;
  updateColumn: (
    prop:
      | string
      | Array<{
          prop: string;
          updates: Partial<ColumnOption<T>>;
        }>,
    updates?: Partial<ColumnOption<T>>,
  ) => void;
  batchUpdateColumns: (
    updates: Array<{
      prop: string;
      updates: Partial<ColumnOption<T>>;
    }>,
  ) => void;
  reorderColumns: (fromIndex: number, toIndex: number) => void;
  getColumnConfig: (prop: string) => ColumnOption<T> | undefined;
  getAllColumns: () => ColumnOption<T>[];
  resetColumns: () => void;
}
export function useTableColumns<T = any>(
  columnsFactory: () => ColumnOption<T>[],
): {
  columns: any;
  columnChecks: any;
} & DynamicColumnConfig<T> {
  const dynamicColumns = ref<ColumnOption<T>[]>(columnsFactory());
  const columnChecks = ref<ColumnOption<T>[]>(
    getColumnChecks(dynamicColumns.value),
  );
  watch(
    dynamicColumns,
    (newCols) => {
      const visibilityMap = new Map(
        columnChecks.value.map((c) => [
          getColumnKey(c),
          getColumnVisibility(c),
        ]),
      );
      const newChecks = getColumnChecks(newCols).map((c) => {
        const key = getColumnKey(c);
        const visibility = visibilityMap.has(key)
          ? visibilityMap.get(key)
          : getColumnVisibility(c);
        return {
          ...c,
          checked: visibility,
          visible: visibility,
        };
      });
      columnChecks.value = newChecks;
    },
    { deep: true },
  );
  const columns = computed(() => {
    const colMap = new Map(
      dynamicColumns.value.map((c) => [getColumnKey(c), c]),
    );
    return columnChecks.value
      .filter((c) => getColumnVisibility(c))
      .map((c) => colMap.get(getColumnKey(c)))
      .filter(Boolean) as ColumnOption<T>[];
  });
  const setDynamicColumns = (
    updater: (cols: ColumnOption<T>[]) => void | ColumnOption<T>[],
  ) => {
    const copy = [...dynamicColumns.value];
    const result = updater(copy);
    dynamicColumns.value = Array.isArray(result) ? result : copy;
  };
  return {
    columns,
    columnChecks,
    addColumn: (column: ColumnOption<T> | ColumnOption<T>[], index?: number) =>
      setDynamicColumns((cols) => {
        const next = [...cols];
        const columnsToAdd = Array.isArray(column) ? column : [column];
        const insertIndex =
          typeof index === "number" && index >= 0 && index <= next.length
            ? index
            : next.length;
        next.splice(insertIndex, 0, ...columnsToAdd);
        return next;
      }),
    removeColumn: (prop: string | string[]) =>
      setDynamicColumns((cols) => {
        const propsToRemove = Array.isArray(prop) ? prop : [prop];
        return cols.filter((c) => !propsToRemove.includes(getColumnKey(c)));
      }),
    updateColumn: (
      prop:
        | string
        | Array<{
            prop: string;
            updates: Partial<ColumnOption<T>>;
          }>,
      updates?: Partial<ColumnOption<T>>,
    ) => {
      if (Array.isArray(prop)) {
        setDynamicColumns((cols) => {
          const map = new Map(prop.map((u) => [u.prop, u.updates]));
          return cols.map((c) => {
            const key = getColumnKey(c);
            const upd = map.get(key);
            return upd ? { ...c, ...upd } : c;
          });
        });
      } else if (updates) {
        setDynamicColumns((cols) =>
          cols.map((c) =>
            getColumnKey(c) === prop ? { ...c, ...updates } : c,
          ),
        );
      }
    },
    toggleColumn: (prop: string | string[], visible?: boolean) => {
      const propsToToggle = Array.isArray(prop) ? prop : [prop];
      const next = [...columnChecks.value];
      propsToToggle.forEach((p) => {
        const i = next.findIndex((c) => getColumnKey(c) === p);
        if (i > -1) {
          const currentVisibility = getColumnVisibility(next[i]);
          const newVisibility = visible ?? !currentVisibility;
          next[i] = {
            ...next[i],
            checked: newVisibility,
            visible: newVisibility,
          };
        }
      });
      columnChecks.value = next;
    },
    resetColumns: () => {
      dynamicColumns.value = columnsFactory();
    },
    batchUpdateColumns: (updates) =>
      setDynamicColumns((cols) => {
        const map = new Map(updates.map((u) => [u.prop, u.updates]));
        return cols.map((c) => {
          const key = getColumnKey(c);
          const upd = map.get(key);
          return upd ? { ...c, ...upd } : c;
        });
      }),
    reorderColumns: (fromIndex: number, toIndex: number) =>
      setDynamicColumns((cols) => {
        if (
          fromIndex < 0 ||
          fromIndex >= cols.length ||
          toIndex < 0 ||
          toIndex >= cols.length ||
          fromIndex === toIndex
        ) {
          return cols;
        }
        const next = [...cols];
        const [moved] = next.splice(fromIndex, 1);
        next.splice(toIndex, 0, moved);
        return next;
      }),
    getColumnConfig: (prop: string) =>
      dynamicColumns.value.find((c) => getColumnKey(c) === prop),
    getAllColumns: () => [...dynamicColumns.value],
  };
}
