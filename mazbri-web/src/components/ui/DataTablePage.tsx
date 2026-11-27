import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  flexRender,
  rowSelectionFeature,
  tableFeatures,
  useTable,
  type ColumnDef,
  type RowData,
  type RowSelectionState,
  type Updater,
} from "@tanstack/react-table";
import { Loader2 } from "lucide-react";
import { cn } from "./cn";
import { Card, EmptyState, PageHeader, Pagination } from "./primitives";
import { useUiTx } from "./tx";

const dataTableFeatures = tableFeatures({
  rowSelectionFeature,
});

type DataTableFeatures = typeof dataTableFeatures;

export interface DataTableColumn<T> {
  readonly id: string;
  readonly header: React.ReactNode;
  readonly accessorKey?: keyof T & string;
  readonly accessorFn?: (row: T) => unknown;
  readonly cell?: (row: T, index: number) => React.ReactNode;
  readonly align?: "left" | "center" | "right";
  readonly width?: string | number;
  readonly minWidth?: string | number;
  readonly className?: string;
  readonly headerClassName?: string;
  readonly mono?: boolean;
}

export interface DataTablePageProps<T> {
  title?: React.ReactNode;
  subtitle?: React.ReactNode;
  headerBadges?: React.ReactNode;
  headerActions?: React.ReactNode;
  columns: readonly DataTableColumn<T>[];
  data: readonly T[];
  getRowId?: (row: T, index: number) => string;
  loading?: boolean;
  emptyText?: React.ReactNode;
  emptyAction?: React.ReactNode;
  filters?: React.ReactNode;
  actions?: React.ReactNode;
  batchActions?:
    | React.ReactNode
    | ((selectedIds: string[], clearSelection: () => void) => React.ReactNode);
  selectable?: boolean;
  selectedKeys?: readonly string[];
  onSelectionChange?: (keys: string[]) => void;
  page?: number;
  pageSize?: number;
  total?: number;
  onPageChange?: (page: number) => void;
  pageSizeOptions?: readonly number[];
  onPageSizeChange?: (pageSize: number) => void;
  onRowClick?: (row: T) => void;
  rowClassName?: (row: T) => string;
  className?: string;
  tableClassName?: string;
  testId?: string;
}

function IndeterminateCheckbox({
  checked,
  indeterminate,
  onChange,
  ariaLabel,
}: {
  checked: boolean;
  indeterminate?: boolean;
  onChange: (e: React.ChangeEvent<HTMLInputElement>) => void;
  ariaLabel: string;
}) {
  const ref = useRef<HTMLInputElement | null>(null);
  useEffect(() => {
    if (ref.current) {
      ref.current.indeterminate = Boolean(!checked && indeterminate);
    }
  }, [checked, indeterminate]);

  return (
    <input
      ref={ref}
      type="checkbox"
      checked={checked}
      onChange={onChange}
      onClick={(e) => e.stopPropagation()}
      aria-label={ariaLabel}
    />
  );
}

export function DataTablePage<T extends RowData>({
  title,
  subtitle,
  headerBadges,
  headerActions,
  columns,
  data,
  getRowId,
  loading = false,
  emptyText,
  emptyAction,
  filters,
  actions,
  batchActions,
  selectable = false,
  selectedKeys,
  onSelectionChange,
  page,
  pageSize,
  total,
  onPageChange,
  pageSizeOptions,
  onPageSizeChange,
  onRowClick,
  rowClassName,
  className,
  tableClassName,
  testId,
}: DataTablePageProps<T>) {
  const { tx } = useUiTx();
  const [internalSelection, setInternalSelection] = useState<RowSelectionState>(
    {},
  );

  const rowSelection = useMemo<RowSelectionState>(() => {
    if (selectedKeys === undefined) return internalSelection;
    const map: RowSelectionState = {};
    for (const k of selectedKeys) {
      map[k] = true;
    }
    return map;
  }, [selectedKeys, internalSelection]);

  const handleRowSelectionChange = useCallback(
    (updater: Updater<RowSelectionState>) => {
      const next =
        typeof updater === "function" ? updater(rowSelection) : updater;
      if (selectedKeys === undefined) {
        setInternalSelection(next);
      }
      if (onSelectionChange) {
        const ids = Object.keys(next).filter((k) => Boolean(next[k]));
        onSelectionChange(ids);
      }
    },
    [rowSelection, selectedKeys, onSelectionChange],
  );

  const selectedIds = useMemo(
    () => Object.keys(rowSelection).filter((k) => Boolean(rowSelection[k])),
    [rowSelection],
  );

  const clearSelection = useCallback(() => {
    handleRowSelectionChange({});
  }, [handleRowSelectionChange]);

  const tableData = useMemo(() => [...data], [data]);

  const columnDefs = useMemo<ColumnDef<DataTableFeatures, T, unknown>[]>(() => {
    const defs: ColumnDef<DataTableFeatures, T, unknown>[] = [];
    if (selectable) {
      defs.push({
        id: "__select__",
        header: ({ table }) => (
          <IndeterminateCheckbox
            checked={table.getIsAllRowsSelected()}
            indeterminate={table.getIsSomeRowsSelected()}
            onChange={table.getToggleAllRowsSelectedHandler()}
            ariaLabel={tx("全选", "Select all")}
          />
        ),
        cell: ({ row }) => (
          <IndeterminateCheckbox
            checked={row.getIsSelected()}
            indeterminate={row.getIsSomeSelected()}
            onChange={row.getToggleSelectedHandler()}
            ariaLabel={tx("选择行", "Select row")}
          />
        ),
      });
    }

    for (const col of columns) {
      defs.push({
        id: col.id,
        header: () => col.header,
        accessorFn: col.accessorFn
          ? (row) => col.accessorFn?.(row)
          : col.accessorKey
            ? (row) => (row as Record<string, unknown>)[col.accessorKey!]
            : undefined,
        cell: ({ row, getValue }) => {
          if (col.cell) {
            return col.cell(row.original, row.index);
          }
          const val = getValue();
          if (val === null || val === undefined || val === "") {
            return <span className="text-muted-foreground">—</span>;
          }
          return String(val);
        },
      });
    }
    return defs;
  }, [columns, selectable, tx]);

  const colMetaById = useMemo(() => {
    const map = new Map<string, DataTableColumn<T>>();
    for (const col of columns) {
      map.set(col.id, col);
    }
    return map;
  }, [columns]);

  const table = useTable(
    {
      features: dataTableFeatures,
      data: tableData,
      columns: columnDefs,
      enableRowSelection: selectable,
      getRowId: (row, index) => (getRowId ? getRowId(row, index) : String(index)),
      state: {
        rowSelection,
      },
      onRowSelectionChange: handleRowSelectionChange,
    },
    (state) => ({
      rowSelection: state.rowSelection,
    }),
  );

  const hasSelection = selectable && selectedIds.length > 0;
  const resolvedBatchActions =
    hasSelection && batchActions
      ? typeof batchActions === "function"
        ? batchActions(selectedIds, clearSelection)
        : batchActions
      : null;

  const hasToolbar = Boolean(filters || actions || resolvedBatchActions);
  const showPagination =
    typeof page === "number" &&
    typeof pageSize === "number" &&
    typeof total === "number" &&
    typeof onPageChange === "function";

  const rows = table.getRowModel().rows;
  const headerGroups = table.getHeaderGroups();

  return (
    <div data-testid={testId} className={cn("space-y-4 min-w-0", className)}>
      {title && (
        <PageHeader
          title={title}
          subtitle={subtitle}
          badges={headerBadges}
          actions={headerActions}
        />
      )}

      <Card className="overflow-hidden">
        {hasToolbar && (
          <div className="flex flex-wrap items-center justify-between gap-2.5 border-b border-border px-4 py-3">
            <div className="flex flex-wrap items-center gap-2 min-w-0">
              {filters}
              {hasSelection && (
                <span className="inline-flex items-center gap-1.5 rounded-md bg-primary-soft px-2 py-1 text-xs font-medium text-primary">
                  {tx(
                    `已选 ${selectedIds.length} 项`,
                    `${selectedIds.length} selected`,
                  )}
                </span>
              )}
              {resolvedBatchActions}
            </div>
            {actions && (
              <div className="flex flex-wrap items-center gap-2 shrink-0 ml-auto">
                {actions}
              </div>
            )}
          </div>
        )}

        <div className="relative overflow-x-auto">
          {loading && rows.length > 0 && (
            <div className="absolute inset-0 z-10 flex items-center justify-center bg-card/65 backdrop-blur-[1px]">
              <Loader2 className="h-5 w-5 animate-spin text-primary" />
            </div>
          )}

          {loading && rows.length === 0 ? (
            <div className="flex items-center justify-center gap-2 py-14 text-xs text-muted-foreground">
              <Loader2 className="h-4 w-4 animate-spin text-primary" />
              <span>{tx("加载中…", "Loading…")}</span>
            </div>
          ) : rows.length === 0 ? (
            <div className="p-4">
              <EmptyState
                title={emptyText ?? tx("暂无数据", "No data")}
                action={emptyAction}
              />
            </div>
          ) : (
            <table
              className={cn(
                "w-full border-collapse text-xs text-foreground",
                tableClassName,
              )}
            >
              <thead>
                {headerGroups.map((headerGroup) => (
                  <tr
                    key={headerGroup.id}
                    className="border-b border-border bg-muted/45 text-muted-foreground"
                  >
                    {headerGroup.headers.map((header) => {
                      const colId = header.column.id;
                      const meta = colMetaById.get(colId);
                      const isSelectCol = colId === "__select__";
                      const align = meta?.align ?? "left";
                      return (
                        <th
                          key={header.id}
                          style={{
                            width: isSelectCol ? 40 : meta?.width,
                            minWidth: meta?.minWidth,
                          }}
                          className={cn(
                            "h-9 px-3 font-medium whitespace-nowrap select-none",
                            isSelectCol && "w-10 text-center px-3",
                            !isSelectCol && align === "left" && "text-left",
                            !isSelectCol && align === "center" && "text-center",
                            !isSelectCol &&
                              align === "right" &&
                              "text-right pr-6",
                            meta?.headerClassName,
                          )}
                        >
                          {header.isPlaceholder
                            ? null
                            : flexRender(
                                header.column.columnDef.header,
                                header.getContext(),
                              )}
                        </th>
                      );
                    })}
                  </tr>
                ))}
              </thead>
              <tbody className="divide-y divide-border-subtle">
                {rows.map((row) => {
                  const selected = selectable && row.getIsSelected();
                  return (
                    <tr
                      key={row.id}
                      onClick={
                        onRowClick ? () => onRowClick(row.original) : undefined
                      }
                      className={cn(
                        "transition-colors hover:bg-hover-bg/75",
                        selected && "bg-primary-soft/50 hover:bg-primary-soft/75",
                        onRowClick && "cursor-pointer",
                        rowClassName?.(row.original),
                      )}
                    >
                      {row.getAllCells().map((cell) => {
                        const colId = cell.column.id;
                        const meta = colMetaById.get(colId);
                        const isSelectCol = colId === "__select__";
                        const align = meta?.align ?? "left";
                        return (
                          <td
                            key={cell.id}
                            className={cn(
                              "px-3 py-2.5 align-middle",
                              isSelectCol && "w-10 text-center px-3",
                              !isSelectCol && align === "left" && "text-left",
                              !isSelectCol &&
                                align === "center" &&
                                "text-center",
                              !isSelectCol &&
                                align === "right" &&
                                "text-right pr-6 tabular-nums",
                              meta?.mono && "font-mono-tech",
                              meta?.className,
                            )}
                          >
                            {flexRender(
                              cell.column.columnDef.cell,
                              cell.getContext(),
                            )}
                          </td>
                        );
                      })}
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
        </div>

        {showPagination && (
          <div className="border-t border-border px-4 py-2.5">
            <Pagination
              page={page}
              pageSize={pageSize}
              total={total}
              onPageChange={onPageChange}
              pageSizeOptions={pageSizeOptions}
              onPageSizeChange={onPageSizeChange}
            />
          </div>
        )}
      </Card>
    </div>
  );
}
