import React, { useEffect, useMemo, useRef, useState } from "react";
import {
  Badge,
  Card,
  DataTablePage,
  type DataTableColumn,
} from "@/components/ui";
import { listSloStatus } from "@/api/opsReliability";
import { useT } from "@/locales";
import {
  buildSloTableColumns,
  formatBudgetRemaining,
  hasSlimErrorBudget,
  hasSloBudgetData,
  mergeSloBudgetRows,
  takeSloTopViolators,
  type SloBudgetRow,
} from "@/utils/ops/slo";
import { showErrorToast } from "@/utils/ui/message-guard";
import { useOpsRole } from "../shared/opsHelpers";

const DETAIL_LIMIT = 8;

export interface SloBudgetPanelProps {
  connected?: boolean;
  remaining?: number | null;
  sloName?: string | null;
  reloadTick: number;
}

export function SloBudgetPanel({
  connected,
  remaining,
  sloName,
  reloadTick,
}: SloBudgetPanelProps) {
  const { t } = useT();
  const { isAdmin } = useOpsRole();
  const [loading, setLoading] = useState(false);
  const [adminRows, setAdminRows] = useState<SloBudgetRow[]>([]);
  const seqRef = useRef(0);

  useEffect(() => {
    if (!isAdmin) {
      setAdminRows([]);
      return;
    }
    const mine = ++seqRef.current;
    setLoading(true);
    listSloStatus()
      .then((data) => {
        if (mine !== seqRef.current) return;
        setAdminRows(mergeSloBudgetRows(data?.definitions, data?.status));
      })
      .catch((error: unknown) => {
        if (mine !== seqRef.current) return;
        setAdminRows([]);
        showErrorToast(
          error instanceof Error
            ? error.message
            : t("ops.overview.sloBudgetLoadFailed"),
        );
      })
      .finally(() => {
        if (mine === seqRef.current) setLoading(false);
      });
  }, [isAdmin, reloadTick, t]);

  const slimVisible = hasSlimErrorBudget(connected, remaining);
  const adminVisible = hasSloBudgetData(adminRows);
  const showPanel = isAdmin ? adminVisible : slimVisible;

  const detailRows = useMemo(
    () => takeSloTopViolators(adminRows, DETAIL_LIMIT),
    [adminRows],
  );

  const columns = useMemo<DataTableColumn<SloBudgetRow>[]>(() => {
    const rawCols = buildSloTableColumns(t, { adminDetail: true });
    return rawCols.map((c) => ({
      id: c.prop,
      header: c.label,
      width: c.width,
      minWidth: c.minWidth,
      align: c.align ?? "left",
      mono: c.prop === "ts",
      cell: (row) => {
        if (!c.formatter) {
          const val = row[c.prop];
          return val === null || val === undefined || val === "" ? "—" : String(val);
        }
        const out = c.formatter(row);
        if (typeof out === "string") return out;
        return (
          <span
            className={
              out.tone === "danger"
                ? "font-medium text-danger-fg"
                : "text-muted-foreground"
            }
          >
            {out.children}
          </span>
        );
      },
    }));
  }, [t]);

  if (!showPanel) return null;

  const resolvedName = sloName?.trim() || t("ops.overview.sloBudgetDefaultName");

  return (
    <Card className="overflow-hidden">
      <div className="flex items-center justify-between border-b border-border px-4 py-3">
        <h3 className="text-xs font-semibold text-foreground">
          {t("ops.overview.sloBudgetTitle")}
        </h3>
      </div>

      {!isAdmin ? (
        <div className="p-4">
          <Badge variant="success">
            {t("ops.overview.sloBudgetSlim", {
              name: resolvedName,
              remaining: formatBudgetRemaining(remaining),
            })}
          </Badge>
        </div>
      ) : (
        <DataTablePage
          columns={columns}
          data={detailRows}
          getRowId={(r) => r.name}
          loading={loading}
          className="[&>div]:border-0 [&>div]:rounded-none"
        />
      )}
    </Card>
  );
}
