import { useElementSize } from "@vueuse/core";
import { toValue, watch, type MaybeRefOrGetter, type Ref } from "vue";
import type { ColumnOption } from "@/types/component";

export interface AdaptiveTableColumnTextContext<T> {
  column: ColumnOption<T>;
  columnKey: string;
  row: T;
  rowIndex: number;
}

export interface UseAdaptiveTableColumnsOptions<T> {
  columns: Ref<ColumnOption<T>[]>;
  containerRef: Ref<HTMLElement | null>;
  rows: MaybeRefOrGetter<T[]>;
  baseColumns: MaybeRefOrGetter<ColumnOption<T>[]>;
  enabled?: MaybeRefOrGetter<boolean>;
  sampleSize?: number;
  cellPadding?: number;
  selectionWidth?: number;
  operationsWidth?: number;




  hardMinWidth?: number;





  softMinCap?: number;




  defaultMaxWidth?: number;





  fitContainer?: boolean;
  getCellText?: (context: AdaptiveTableColumnTextContext<T>) => string;
  getColumnWeight?: (context: {
    column: ColumnOption<T>;
    columnKey: string;
    width: number;

    mode: "grow" | "shrink";
  }) => number;
}

const DEFAULT_CELL_PADDING = 28;
const DEFAULT_SAMPLE_SIZE = 50;
const DEFAULT_SELECTION_WIDTH = 48;
const DEFAULT_OPERATIONS_WIDTH = 88;
const DEFAULT_HARD_MIN = 72;
const DEFAULT_SOFT_MIN_CAP = 168;
const DEFAULT_MAX_WIDTH = 260;

type MutableColumn<T> = ColumnOption<T> & {
  width?: number;
  minWidth?: number;
  maxWidth?: number;
  showOverflowTooltip?: boolean;
};

const measureTextWidth = (() => {
  let canvas: HTMLCanvasElement | null = null;
  let context: CanvasRenderingContext2D | null = null;

  return (value: string) => {
    if (!value) return 0;
    if (typeof document === "undefined") {
      return value.length * 8;
    }
    if (!canvas) {
      canvas = document.createElement("canvas");
      context = canvas.getContext("2d");
      if (context) {
        const bodyFont = document.body
          ? getComputedStyle(document.body).font
          : "";
        context.font = bodyFont || "14px sans-serif";
      }
    }
    if (!context) {
      return value.length * 8;
    }
    return context.measureText(value).width;
  };
})();

const normalizeDisplayValue = (value: unknown): string => {
  if (value == null || value === "") return "-";
  if (typeof value === "string") return value;
  if (
    typeof value === "number" ||
    typeof value === "bigint" ||
    typeof value === "boolean"
  ) {
    return String(value);
  }
  if (Array.isArray(value)) {
    return value.map((item) => normalizeDisplayValue(item)).join(", ");
  }
  if (value instanceof Date) {
    return value.toISOString();
  }
  if (typeof value === "object") {
    try {
      return JSON.stringify(value);
    } catch {
      return String(value);
    }
  }
  return String(value);
};

const toWidthNumber = (value: unknown): number => {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === "string") {
    const parsed = Number.parseFloat(value);
    return Number.isFinite(parsed) ? parsed : 0;
  }
  return 0;
};

const getColumnKey = <T>(column: ColumnOption<T>) =>
  String(column.prop ?? column.slotName ?? column.type ?? column.label ?? "");

const resolveColumnText = <T>(
  context: AdaptiveTableColumnTextContext<T>,
  getCellText?: (context: AdaptiveTableColumnTextContext<T>) => string,
) => {
  if (getCellText) {
    return getCellText(context);
  }

  const { column, row, columnKey } = context;
  if (column.formatter) {
    return normalizeDisplayValue(column.formatter(row));
  }
  const prop = column.prop as keyof T | undefined;
  if (prop && row && typeof row === "object") {
    const value = (row as Record<string, unknown>)[String(prop)];
    return normalizeDisplayValue(value);
  }
  if (columnKey === "operations") {
    return "";
  }
  return normalizeDisplayValue(column.label ?? "");
};

const isFixedWidthColumn = (columnKey: string) =>
  ["selection", "expand", "index", "globalIndex", "operations"].includes(
    columnKey,
  );

const isOperationsColumn = <T>(column: ColumnOption<T>, columnKey: string) =>
  columnKey === "operations" ||
  column.slotName === "operations" ||
  column.prop === "operations";

const isFlexibleColumn = <T>(column: ColumnOption<T>, columnKey: string) =>
  !isFixedWidthColumn(columnKey) &&
  !isOperationsColumn(column, columnKey) &&
  column.fixed !== "left" &&
  column.fixed !== "right";





const orderColumnsForLayout = <T>(columns: MutableColumn<T>[]) => {
  const selection: MutableColumn<T>[] = [];
  const operations: MutableColumn<T>[] = [];
  const middle: MutableColumn<T>[] = [];
  for (const column of columns) {
    const key = getColumnKey(column);
    if (key === "selection" || column.type === "selection") {
      selection.push(column);
    } else if (isOperationsColumn(column, key)) {
      operations.push(column);
    } else {
      middle.push(column);
    }
  }
  return [...selection, ...middle, ...operations];
};






const defaultMaxForKey = (columnKey: string, fallback: number): number => {
  const key = columnKey.toLowerCase();
  if (
    key.includes("serial") ||
    key.includes("request_id") ||
    key.includes("uuid")
  ) {

    return 340;
  }
  if (key.endsWith("_id") || key === "id") {
    return 280;
  }
  if (
    key.includes("status") ||
    key.includes("enabled") ||
    key === "code" ||
    key.endsWith("_code")
  ) {
    return 110;
  }
  if (
    key.includes("duration") ||
    key.includes("latency") ||
    key.endsWith("_ms") ||
    key.includes("timeout")
  ) {
    return 100;
  }
  if (
    key.includes("time") ||
    key.includes("created") ||
    key.includes("updated") ||
    key.includes("at")
  ) {
    return 150;
  }
  if (key.includes("ip")) {
    return 136;
  }

  if (key.includes("channel")) {
    return 200;
  }
  if (key.includes("name") || key.includes("tenant")) {
    return 180;
  }
  return fallback;
};

const softMinCapForKey = (columnKey: string, softMinCap: number): number => {
  const key = columnKey.toLowerCase();

  if (
    key.includes("serial") ||
    key.includes("request_id") ||
    key.includes("uuid")
  ) {
    return Math.max(softMinCap, 320);
  }
  if (key.includes("time") || key.includes("created") || key.includes("updated")) {
    return Math.min(softMinCap, 140);
  }
  if (
    key.includes("duration") ||
    key.includes("latency") ||
    key.endsWith("_ms") ||
    key.includes("status")
  ) {
    return Math.min(softMinCap, 100);
  }
  if (key.includes("ip")) {
    return Math.min(softMinCap, 128);
  }
  return softMinCap;
};

const clamp = (value: number, min: number, max: number) =>
  Math.min(Math.max(value, min), max);

const resolveEffectiveMin = <T>(
  column: ColumnOption<T>,
  columnKey: string,
  hardMin: number,
  softMinCap: number,
  effectiveMax: number,
) => {
  const declared = Math.max(
    toWidthNumber(column.width),
    toWidthNumber(column.minWidth),
  );
  if (declared <= 0) {
    return Math.min(hardMin, effectiveMax);
  }

  const cap = softMinCap > 0 ? softMinCapForKey(columnKey, softMinCap) : declared;
  const floored = Math.max(hardMin, Math.min(declared, cap));
  return Math.min(floored, effectiveMax);
};

const resolveEffectiveMax = <T>(
  column: ColumnOption<T>,
  columnKey: string,
  defaultMaxWidth: number,
) => {
  const explicitMax = toWidthNumber((column as MutableColumn<T>).maxWidth);
  if (explicitMax > 0) {
    return explicitMax;
  }
  return defaultMaxForKey(columnKey, defaultMaxWidth);
};

const defaultWeightForKey = (
  columnKey: string,
  width: number,
  mode: "grow" | "shrink",
) => {
  const key = columnKey.toLowerCase();

  if (
    mode === "shrink" &&
    (key.includes("serial") ||
      key.includes("request_id") ||
      key.includes("uuid"))
  ) {
    return 0.12;
  }
  if (
    mode === "shrink" &&
    (key.includes("channel") ||
      key.includes("tenant") ||
      key.includes("name") ||
      key.includes("result"))
  ) {
    return Math.max(width / 80, 1.6);
  }
  return Math.max(width / 120, 1);
};

const distributeDelta = <T>(
  columns: MutableColumn<T>[],
  deltaTotal: number,
  getColumnWeight:
    | ((context: {
        column: ColumnOption<T>;
        columnKey: string;
        width: number;
        mode: "grow" | "shrink";
      }) => number)
    | undefined,
  mode: "grow" | "shrink",
  minOf: (column: MutableColumn<T>, key: string) => number,
  maxOf: (column: MutableColumn<T>, key: string) => number,
) => {
  if (deltaTotal === 0 || columns.length === 0) {
    return;
  }

  let remaining = deltaTotal;

  for (let pass = 0; pass < 8 && Math.abs(remaining) > 0; pass += 1) {
    const candidates = columns.filter((column) => {
      const key = getColumnKey(column);
      const width = toWidthNumber(column.width);
      if (mode === "grow") {
        return width < maxOf(column, key);
      }
      return width > minOf(column, key);
    });
    if (candidates.length === 0) {
      break;
    }

    const totalWeight = candidates.reduce((sum, column) => {
      const columnKey = getColumnKey(column);
      const width = toWidthNumber(column.width);
      const weight =
        getColumnWeight?.({ column, columnKey, width, mode }) ??
        defaultWeightForKey(columnKey, width, mode);
      return sum + Math.max(weight, 0.01);
    }, 0);
    if (totalWeight <= 0) {
      break;
    }

    let consumed = 0;
    candidates.forEach((column, index) => {
      const columnKey = getColumnKey(column);
      const width = toWidthNumber(column.width);
      const weight =
        getColumnWeight?.({ column, columnKey, width, mode }) ??
        defaultWeightForKey(columnKey, width, mode);
      const safeWeight = Math.max(weight, 0.01);
      const rawShare =
        index === candidates.length - 1
          ? remaining - consumed
          : Math.trunc((remaining * safeWeight) / totalWeight);
      const minW = minOf(column, columnKey);
      const maxW = maxOf(column, columnKey);
      const next =
        mode === "grow"
          ? Math.min(maxW, width + rawShare)
          : Math.max(minW, width + rawShare);
      const applied = next - width;
      column.width = next;
      column.minWidth = next;
      consumed += applied;
    });

    if (consumed === 0) {
      break;
    }
    remaining -= consumed;
  }
};

export const buildAdaptiveColumns = <T>(
  baseColumns: ColumnOption<T>[],
  rows: T[],
  getCellText:
    | ((context: AdaptiveTableColumnTextContext<T>) => string)
    | undefined,
  sampleSize: number,
  cellPadding: number,
  containerWidth: number,
  selectionWidth: number,
  operationsWidth: number,
  getColumnWeight:
    | ((context: {
        column: ColumnOption<T>;
        columnKey: string;
        width: number;
        mode: "grow" | "shrink";
      }) => number)
    | undefined,
  hardMinWidth: number,
  softMinCap: number,
  defaultMaxWidth: number,
  fitContainer: boolean,
): MutableColumn<T>[] => {
  const sampleRows = rows.slice(0, sampleSize);
  const minMap = new Map<string, number>();
  const maxMap = new Map<string, number>();

  const computedColumns: MutableColumn<T>[] = baseColumns.map((column) => {
    const columnKey = getColumnKey(column);
    if (columnKey === "selection" || column.type === "selection") {
      minMap.set(columnKey, selectionWidth);
      maxMap.set(columnKey, selectionWidth);
      return {
        ...column,
        width: selectionWidth,
        minWidth: selectionWidth,
        showOverflowTooltip: false,
      };
    }
    if (isOperationsColumn(column, columnKey)) {
      const opWidth =
        toWidthNumber(column.width) > 0
          ? toWidthNumber(column.width)
          : operationsWidth;
      minMap.set(columnKey, opWidth);
      maxMap.set(columnKey, opWidth);
      return {
        ...column,

        fixed: "right",
        width: opWidth,
        minWidth: opWidth,
        showOverflowTooltip: false,
      };
    }

    const effectiveMax = resolveEffectiveMax(
      column,
      columnKey,
      defaultMaxWidth,
    );
    const effectiveMin = resolveEffectiveMin(
      column,
      columnKey,
      hardMinWidth,
      softMinCap,
      effectiveMax,
    );
    minMap.set(columnKey, effectiveMin);
    maxMap.set(columnKey, Math.max(effectiveMin, effectiveMax));

    const headerWidth =
      measureTextWidth(String(column.label ?? "")) + cellPadding;
    const contentWidth = sampleRows.reduce((maxWidth, row, rowIndex) => {
      const text = resolveColumnText(
        { column, columnKey, row, rowIndex },
        getCellText,
      );
      return Math.max(maxWidth, measureTextWidth(text) + cellPadding);
    }, headerWidth);


    const ideal = clamp(Math.ceil(contentWidth), effectiveMin, effectiveMax);
    return {
      ...column,
      width: ideal,
      minWidth: ideal,
      maxWidth: effectiveMax,

      showOverflowTooltip:
        (column as MutableColumn<T>).showOverflowTooltip ?? true,
    };
  });

  const sumWidths = () =>
    computedColumns.reduce((sum, column) => sum + toWidthNumber(column.width), 0);

  let totalWidth = sumWidths();
  const usableContainer =
    containerWidth > 0 ? Math.floor(containerWidth) : totalWidth;


  if (usableContainer > totalWidth) {
    const flexible = computedColumns.filter((column) =>
      isFlexibleColumn(column, getColumnKey(column)),
    );
    distributeDelta(
      flexible,
      usableContainer - totalWidth,
      getColumnWeight,
      "grow",
      (col, key) => minMap.get(key) ?? hardMinWidth,
      (col, key) => maxMap.get(key) ?? defaultMaxWidth,
    );
    totalWidth = sumWidths();
  }


  if (fitContainer && usableContainer > 0 && totalWidth > usableContainer) {
    const flexible = computedColumns.filter((column) =>
      isFlexibleColumn(column, getColumnKey(column)),
    );
    distributeDelta(
      flexible,
      usableContainer - totalWidth,
      getColumnWeight,
      "shrink",
      (col, key) => minMap.get(key) ?? hardMinWidth,
      (col, key) => maxMap.get(key) ?? defaultMaxWidth,
    );
    totalWidth = sumWidths();
  }




  totalWidth = sumWidths();
  if (usableContainer > totalWidth) {
    const flexible = computedColumns.filter((column) =>
      isFlexibleColumn(column, getColumnKey(column)),
    );
    const isBadResidualFiller = (key: string) => {
      const k = key.toLowerCase();
      return (
        k.includes("serial") ||
        k.includes("request_id") ||
        k.includes("uuid") ||
        k.includes("channel") ||
        k.includes("tenant") ||
        (k.includes("name") && !k.includes("time")) ||
        k.includes("result") ||
        k.includes("supplier")
      );
    };
    const filler =
      [...flexible].reverse().find((c) => !isBadResidualFiller(getColumnKey(c))) ??
      flexible[flexible.length - 1];
    if (filler) {
      const extra = usableContainer - totalWidth;
      const next = toWidthNumber(filler.width) + extra;
      filler.width = next;
      filler.minWidth = next;

      maxMap.set(
        getColumnKey(filler),
        Math.max(maxMap.get(getColumnKey(filler)) ?? 0, next),
      );
    }
  }


  computedColumns.forEach((column) => {
    const w = toWidthNumber(column.width);
    column.width = w;
    column.minWidth = w;
    if (isOperationsColumn(column, getColumnKey(column))) {
      column.fixed = "right";
    }
  });

  return orderColumnsForLayout(computedColumns);
};

export const useAdaptiveTableColumns = <T>(
  options: UseAdaptiveTableColumnsOptions<T>,
) => {
  const { width: containerWidth } = useElementSize(options.containerRef);

  const syncColumns = () => {
    const previousColumns = new Map(
      options.columns.value.map((column) => [getColumnKey(column), column]),
    );
    const nextColumns = buildAdaptiveColumns(
      toValue(options.baseColumns) ?? [],
      toValue(options.rows) ?? [],
      options.getCellText,
      options.sampleSize ?? DEFAULT_SAMPLE_SIZE,
      options.cellPadding ?? DEFAULT_CELL_PADDING,
      containerWidth.value,
      options.selectionWidth ?? DEFAULT_SELECTION_WIDTH,
      options.operationsWidth ?? DEFAULT_OPERATIONS_WIDTH,
      options.getColumnWeight,
      options.hardMinWidth ?? DEFAULT_HARD_MIN,
      options.softMinCap ?? DEFAULT_SOFT_MIN_CAP,
      options.defaultMaxWidth ?? DEFAULT_MAX_WIDTH,
      options.fitContainer ?? true,
    );

    options.columns.value = nextColumns.map((column) => {
      const previous = previousColumns.get(getColumnKey(column));
      if (!previous) {
        return column;
      }
      return {
        ...column,
        visible: previous.visible ?? column.visible,
        checked: previous.checked ?? column.checked,
      };
    });
  };

  watch(
    [
      () => containerWidth.value,
      () => toValue(options.baseColumns),
      () => toValue(options.rows),
      () => toValue(options.enabled) ?? true,
    ],
    ([, , , enabled]) => {
      if (enabled === false) return;
      syncColumns();
    },
    { immediate: true, deep: false },
  );

  return {
    syncAdaptiveColumns: syncColumns,
    containerWidth,
  };
};
