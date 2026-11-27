import React, { useEffect, useMemo, useRef, useState } from "react";
import {
  Card,
  DataTablePage,
  Drawer,
  StatusBadge,
  type DataTableColumn,
} from "@/components/ui";
import { listAnomalies, type AnomalyEvent } from "@/api/opsReliability";
import { useT } from "@/locales";
import {
  ANOMALY_COLUMN_DEFS,
  anomalyFieldDisplay,
  buildAnomalyDrawerFields,
  normalizeAnomalyEvents,
  takeAnomalyTop,
} from "@/utils/ops/anomaly";
import { showErrorToast } from "@/utils/ui/message-guard";

const LIMIT = 10;

export interface RecentAnomaliesProps {
  window: { start: number; end: number } | null;
  reloadTick: number;
}

export function RecentAnomalies({
  window: win,
  reloadTick,
}: RecentAnomaliesProps) {
  const { t } = useT();
  const [loading, setLoading] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [rows, setRows] = useState<AnomalyEvent[]>([]);
  const [selected, setSelected] = useState<AnomalyEvent | null>(null);
  const seqRef = useRef(0);

  useEffect(() => {
    if (!win || win.end <= win.start) return;
    const mine = ++seqRef.current;
    setLoading(true);
    listAnomalies({ start: win.start, end: win.end, limit: LIMIT })
      .then((raw) => {
        if (mine !== seqRef.current) return;
        setRows(takeAnomalyTop(normalizeAnomalyEvents(raw), LIMIT));
      })
      .catch((error: unknown) => {
        if (mine !== seqRef.current) return;
        setRows([]);
        showErrorToast(
          error instanceof Error
            ? error.message
            : t("ops.overview.anomalyLoadFailed"),
        );
      })
      .finally(() => {
        if (mine === seqRef.current) {
          setLoading(false);
          setLoaded(true);
        }
      });
  }, [win?.start, win?.end, reloadTick, t]);

  const columns = useMemo<DataTableColumn<AnomalyEvent>[]>(
    () =>
      ANOMALY_COLUMN_DEFS.filter((d) => !d.drawerOnly).map((d) => ({
        id: d.prop,
        header: t(d.labelKey),
        width: d.width,
        align:
          d.prop === "value" || d.prop === "baseline" || d.prop === "score"
            ? "right"
            : "left",
        mono: d.prop === "detected_at" || d.prop === "entity_id",
        cell: (row) => {
          if (d.prop === "severity" || d.prop === "status") {
            return <StatusBadge status={String(row[d.prop] || "")} />;
          }
          return anomalyFieldDisplay(row, d.prop);
        },
      })),
    [t],
  );

  const drawerFields = useMemo(() => buildAnomalyDrawerFields(t), [t]);

  return (
    <Card className="overflow-hidden">
      <div className="flex items-center justify-between border-b border-border px-4 py-3">
        <h3 className="text-xs font-semibold text-foreground">
          {t("ops.overview.recentAnomalies")}
        </h3>
      </div>

      {loaded && !loading && rows.length === 0 ? (
        <div className="px-4 py-3 text-xs text-muted-foreground">
          {t("ops.overview.anomalyEmpty")}
        </div>
      ) : (
        <DataTablePage
          columns={columns}
          data={rows}
          getRowId={(r, i) => r.id || String(i)}
          loading={loading}
          emptyText={t("ops.overview.anomalyEmpty")}
          onRowClick={(row) => setSelected(row)}
          className="[&>div]:border-0 [&>div]:rounded-none"
        />
      )}

      <Drawer
        open={Boolean(selected)}
        onClose={() => setSelected(null)}
        title={t("ops.overview.anomalyDetail")}
        size="sm"
      >
        {selected && (
          <dl className="divide-y divide-border-subtle rounded-lg border border-border overflow-hidden">
            {drawerFields.map((field) => (
              <div
                key={field.prop}
                className="grid grid-cols-12 gap-2 px-3 py-2 text-xs"
              >
                <dt className="col-span-4 text-muted-foreground">
                  {field.label}
                </dt>
                <dd className="col-span-8 min-w-0 text-foreground break-words">
                  {field.prop === "detail" ? (
                    <pre className="m-0 whitespace-pre-wrap break-words font-mono-tech text-[11px]">
                      {anomalyFieldDisplay(selected, field.prop)}
                    </pre>
                  ) : (
                    anomalyFieldDisplay(selected, field.prop)
                  )}
                </dd>
              </div>
            ))}
          </dl>
        )}
      </Drawer>
    </Card>
  );
}
