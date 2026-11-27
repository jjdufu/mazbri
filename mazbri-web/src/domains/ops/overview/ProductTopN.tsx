import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Info, Search } from "lucide-react";
import {
  Card,
  DataTablePage,
  Input,
  SegmentedControl,
  Select,
  Tooltip,
  type DataTableColumn,
  type SelectOptionItem,
} from "@/components/ui";
import { postOlapQuery } from "@/api/olap";
import { listProductNames, listZeroCallProducts } from "@/api/opsReliability";
import { useT } from "@/locales";
import { formatMoney } from "@/utils/format";
import {
  normalizeProductRankRows,
  productNameMap,
  productRankDelta,
  withProductNames,
  type NamedProductRankRow,
  type ProductRankMetric,
  type ProductRankRow,
  type ZeroCallProductRow,
} from "@/utils/ops/product-rank";
import { showErrorToast } from "@/utils/ui/message-guard";

type ViewMode = "active" | "zero";
const TOP = 10;

export interface ProductTopNProps {
  window: { start: number; end: number } | null;
  reloadTick: number;
}

export function ProductTopN({ window: win, reloadTick }: ProductTopNProps) {
  const { t, locale } = useT();
  const [viewMode, setViewMode] = useState<ViewMode>("active");
  const [metric, setMetric] = useState<ProductRankMetric>("req_count");
  const [loading, setLoading] = useState(false);
  const [rows, setRows] = useState<NamedProductRankRow[]>([]);

  const [zeroLoading, setZeroLoading] = useState(false);
  const [zeroRows, setZeroRows] = useState<ZeroCallProductRow[]>([]);
  const [zeroTotal, setZeroTotal] = useState(0);
  const [zeroBadgeCount, setZeroBadgeCount] = useState(0);
  const [zeroQuery, setZeroQuery] = useState("");
  const [zeroPage, setZeroPage] = useState(1);
  const [zeroPageSize, setZeroPageSize] = useState(10);

  const activeSeqRef = useRef(0);
  const zeroSeqRef = useRef(0);

  const nf = useMemo(() => new Intl.NumberFormat(locale), [locale]);

  const loadActive = useCallback(
    async (
      range: { start: number; end: number },
      nextMetric: ProductRankMetric,
    ) => {
      const mine = ++activeSeqRef.current;
      setLoading(true);
      try {
        const [rankRes, catalog] = await Promise.all([
          postOlapQuery<ProductRankRow>({
            template: "product_rank",
            params: { ...range, metric: nextMetric, limit: TOP },
          }),
          listProductNames(),
        ]);
        if (mine !== activeSeqRef.current) return;
        const ranked = normalizeProductRankRows(rankRes.rows);
        const names = productNameMap(catalog);
        setRows(withProductNames(ranked.slice(0, TOP), names));
      } catch (error) {
        if (mine !== activeSeqRef.current) return;
        setRows([]);
        showErrorToast(
          error instanceof Error
            ? error.message
            : t("ops.overview.productTopLoadFailed"),
        );
      } finally {
        if (mine === activeSeqRef.current) {
          setLoading(false);
        }
      }
    },
    [t],
  );

  const loadZeroPage = useCallback(
    async (
      range: { start: number; end: number },
      page: number,
      pageSize: number,
      queryText: string,
      updateBadge: boolean,
    ) => {
      const mine = ++zeroSeqRef.current;
      setZeroLoading(true);
      try {
        const q = queryText.trim();
        const res = await listZeroCallProducts({
          start: range.start,
          end: range.end,
          page,
          page_size: pageSize,
          ...(q ? { q } : {}),
        });
        if (mine !== zeroSeqRef.current) return;
        setZeroRows(
          (res.items ?? []).map((item) => ({
            product_id: item.product_id,
            name: item.name?.trim() || item.product_id,
          })),
        );
        setZeroTotal(res.total ?? 0);
        if (updateBadge && !q) {
          setZeroBadgeCount(res.total ?? 0);
        }
      } catch (error) {
        if (mine !== zeroSeqRef.current) return;
        setZeroRows([]);
        setZeroTotal(0);
        showErrorToast(
          error instanceof Error
            ? error.message
            : t("ops.overview.productTopLoadFailed"),
        );
      } finally {
        if (mine === zeroSeqRef.current) {
          setZeroLoading(false);
        }
      }
    },
    [t],
  );

  useEffect(() => {
    if (!win || win.end <= win.start) return;
    setZeroPage(1);
    void loadActive(win, metric);
    void loadZeroPage(win, 1, zeroPageSize, zeroQuery, true);
  }, [win?.start, win?.end, reloadTick]);

  const handleMetricChange = (nextMetric: ProductRankMetric) => {
    setMetric(nextMetric);
    if (win) void loadActive(win, nextMetric);
  };

  const handleModeChange = (nextMode: ViewMode) => {
    setViewMode(nextMode);
    if (nextMode === "zero" && win) {
      void loadZeroPage(win, zeroPage, zeroPageSize, zeroQuery, false);
    }
  };

  const handleZeroSearch = () => {
    setZeroPage(1);
    if (win) void loadZeroPage(win, 1, zeroPageSize, zeroQuery, false);
  };

  const segmentOptions = useMemo(
    () => [
      { key: "active" as const, label: t("ops.products.segmentActive") },
      {
        key: "zero" as const,
        label: t("ops.products.segmentZero", { n: zeroBadgeCount }),
      },
    ],
    [t, zeroBadgeCount],
  );

  const metricOptions = useMemo<SelectOptionItem[]>(
    () => [
      {
        value: "req_count",
        label: t("ops.products.metricReq"),
        text: t("ops.products.metricReq"),
      },
      {
        value: "error_rate",
        label: t("ops.products.metricErr"),
        text: t("ops.products.metricErr"),
      },
      {
        value: "p95",
        label: t("ops.products.metricP95"),
        text: t("ops.products.metricP95"),
      },
      {
        value: "cost",
        label: t("ops.products.metricCost"),
        text: t("ops.products.metricCost"),
      },
    ],
    [t],
  );

  const activeColumns = useMemo<DataTableColumn<NamedProductRankRow>[]>(
    () => [
      {
        id: "name",
        header: t("ops.products.colProduct"),
        minWidth: 160,
        cell: (r) => <span className="font-medium text-foreground">{r.name}</span>,
      },
      {
        id: "req_count",
        header: t("ops.products.colReq"),
        width: 120,
        align: "right",
        cell: (r) => nf.format(r.req_count),
      },
      {
        id: "prev_req_count",
        header: t("ops.products.colPrev"),
        width: 120,
        align: "right",
        cell: (r) => nf.format(r.prev_req_count),
      },
      {
        id: "delta",
        header: t("ops.products.colDelta"),
        width: 96,
        align: "right",
        cell: (r) => {
          const d = productRankDelta(r);
          if (d === null) {
            return <span className="text-primary">{t("ops.products.deltaNew")}</span>;
          }
          const sign = d > 0 ? "+" : "";
          return `${sign}${(d * 100).toFixed(1)}%`;
        },
      },
      {
        id: "error_rate",
        header: t("ops.products.colErr"),
        width: 110,
        align: "right",
        cell: (r) => `${(r.error_rate * 100).toFixed(2)}%`,
      },
      {
        id: "p95",
        header: t("ops.products.colP95"),
        width: 110,
        align: "right",
        cell: (r) => r.p95.toFixed(1),
      },
      {
        id: "cost",
        header: t("ops.products.colCost"),
        width: 120,
        align: "right",
        cell: (r) => formatMoney(r.cost),
      },
    ],
    [t, nf],
  );

  const zeroColumns = useMemo<DataTableColumn<ZeroCallProductRow>[]>(
    () => [
      {
        id: "name",
        header: t("ops.products.colName"),
        minWidth: 200,
        cell: (r) => r.name || "—",
      },
      {
        id: "product_id",
        header: t("ops.products.colId"),
        minWidth: 160,
        mono: true,
        cell: (r) => r.product_id || "—",
      },
    ],
    [t],
  );

  return (
    <Card className="overflow-hidden">
      <div className="flex flex-wrap items-center justify-between gap-2.5 border-b border-border px-4 py-3">
        <div className="flex items-center gap-1.5 min-w-0">
          <h3 className="text-xs font-semibold text-foreground">
            {t("ops.overview.productTop")}
          </h3>
          <Tooltip content={t("ops.products.basisNote")}>
            <span className="inline-flex text-muted-foreground hover:text-foreground cursor-help">
              <Info className="h-3.5 w-3.5" />
            </span>
          </Tooltip>
        </div>

        <div className="flex flex-wrap items-center gap-2 shrink-0">
          <SegmentedControl
            items={segmentOptions}
            value={viewMode}
            onChange={handleModeChange}
          />
          {viewMode === "active" ? (
            <Select
              value={metric}
              options={metricOptions}
              onChange={(e) =>
                handleMetricChange(e.target.value as ProductRankMetric)
              }
              className="w-36"
            />
          ) : (
            <div className="relative flex items-center w-52">
              <Search className="pointer-events-none absolute left-2.5 h-3.5 w-3.5 text-muted-foreground" />
              <Input
                value={zeroQuery}
                onChange={(e) => setZeroQuery(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleZeroSearch();
                }}
                placeholder={t("ops.products.zeroSearchPlaceholder")}
                className="pl-8"
              />
            </div>
          )}
        </div>
      </div>

      {viewMode === "active" ? (
        <DataTablePage
          columns={activeColumns}
          data={rows}
          getRowId={(r, i) => `${r.product_id}-${i}`}
          loading={loading}
          emptyText={t("ops.products.rankEmpty")}
          className="[&>div]:border-0 [&>div]:rounded-none"
        />
      ) : (
        <DataTablePage
          columns={zeroColumns}
          data={zeroRows}
          getRowId={(r) => r.product_id}
          loading={zeroLoading}
          emptyText={t("ops.products.zeroEmpty")}
          page={zeroPage}
          pageSize={zeroPageSize}
          total={zeroTotal}
          pageSizeOptions={[10, 20, 50]}
          onPageChange={(nextPage) => {
            setZeroPage(nextPage);
            if (win) void loadZeroPage(win, nextPage, zeroPageSize, zeroQuery, false);
          }}
          onPageSizeChange={(nextSize) => {
            setZeroPageSize(nextSize);
            setZeroPage(1);
            if (win) void loadZeroPage(win, 1, nextSize, zeroQuery, false);
          }}
          className="[&>div]:border-0 [&>div]:rounded-none"
        />
      )}
    </Card>
  );
}
