import type { SloDefinition, SloStatusRow } from "@/api/opsReliability";
import type { ColumnOption } from "@/types/component";
import { formatAlertTime } from "@/utils/ops/alerts-format";

/** SLO 合并行（定义 ∪ 状态）。 */
export interface SloBudgetRow {
  name: string;
  sli_type: string;
  objective: number;
  window_days: number;
  sli: number | null;
  remaining: number | null;
  burn_5m: number | null;
  burn_page: number | null;
  burn_ticket: number | null;
  ts: string | null;
}

export interface SloColumnDef {
  prop: keyof SloBudgetRow;
  labelKey: string;
  width?: number;
  minWidth?: number;
  /** 仅 ADMIN 详表展示（燃烧相关）。 */
  adminOnly?: boolean;
}

export const SLO_COLUMN_DEFS: readonly SloColumnDef[] = [
  { prop: "name", labelKey: "ops.slo.colName", minWidth: 140 },
  { prop: "sli_type", labelKey: "ops.slo.colType", width: 100 },
  { prop: "objective", labelKey: "ops.slo.colObjective", width: 90 },
  { prop: "window_days", labelKey: "ops.slo.colWindow", width: 90 },
  { prop: "sli", labelKey: "ops.slo.colSli", width: 90 },
  { prop: "remaining", labelKey: "ops.slo.colBudget", width: 110 },
  { prop: "burn_5m", labelKey: "ops.slo.colBurn5m", width: 100, adminOnly: true },
  { prop: "burn_page", labelKey: "ops.slo.colBurnPage", width: 80, adminOnly: true },
  { prop: "burn_ticket", labelKey: "ops.slo.colBurnTicket", width: 80, adminOnly: true },
  { prop: "ts", labelKey: "ops.slo.colTs", width: 170 },
];

export type SloTranslate = (key: string) => string;

/** 定义与状态按 name 合并。 */
export function mergeSloBudgetRows(
  definitions: readonly SloDefinition[] | null | undefined,
  status: readonly SloStatusRow[] | null | undefined,
): SloBudgetRow[] {
  const defs = definitions ?? [];
  const byName = new Map((status ?? []).map((s) => [s.name, s]));
  if (defs.length === 0) {
    return (status ?? []).map((s) => ({
      name: String(s.name ?? ""),
      sli_type: "",
      objective: 0,
      window_days: 0,
      sli: s.sli ?? null,
      remaining: s.error_budget_remaining ?? null,
      burn_5m: s.burn_5m ?? null,
      burn_page: s.burn_page ?? null,
      burn_ticket: s.burn_ticket ?? null,
      ts: formatSloTimestamp(s.ts),
    }));
  }
  return defs.map((d) => {
    const s = byName.get(d.name);
    return {
      name: d.name,
      sli_type: d.sli_type,
      objective: d.objective,
      window_days: d.window_days,
      sli: s?.sli ?? null,
      remaining: s?.error_budget_remaining ?? null,
      burn_5m: s?.burn_5m ?? null,
      burn_page: s?.burn_page ?? null,
      burn_ticket: s?.burn_ticket ?? null,
      ts: formatSloTimestamp(s?.ts),
    };
  });
}

/** 是否有可展示的预算数据（无数据则不占位）。 */
export function hasSloBudgetData(
  rows: readonly SloBudgetRow[] | null | undefined,
): boolean {
  return (rows?.length ?? 0) > 0;
}

/** overview 极简块：已接入且有剩余值才展示。 */
export function hasSlimErrorBudget(
  connected: boolean | null | undefined,
  remaining: number | null | undefined,
): boolean {
  return Boolean(connected) && typeof remaining === "number" && Number.isFinite(remaining);
}

/** 违规优先：burn_page > 0 或 remaining 偏低的排前，截 Top N。 */
export function takeSloTopViolators(
  rows: readonly SloBudgetRow[],
  limit = 5,
): SloBudgetRow[] {
  if (limit <= 0 || rows.length === 0) return [];
  const scored = [...rows].sort((a, b) => {
    const aHot =
      (a.burn_page ?? 0) * 1000 +
      (a.burn_ticket ?? 0) * 100 -
      (a.remaining ?? 1) * 10;
    const bHot =
      (b.burn_page ?? 0) * 1000 +
      (b.burn_ticket ?? 0) * 100 -
      (b.remaining ?? 1) * 10;
    return bHot - aHot;
  });
  return scored.slice(0, limit);
}

export function buildSloTableColumns(
  t: SloTranslate,
  options: { adminDetail?: boolean } = {},
): ColumnOption<SloBudgetRow>[] {
  const adminDetail = options.adminDetail ?? true;
  return SLO_COLUMN_DEFS.filter((d) => adminDetail || !d.adminOnly).map(
    (d) => ({
      prop: d.prop,
      label: t(d.labelKey),
      width: d.width,
      minWidth: d.minWidth,
    }),
  );
}

/** SLO 采样时间：ISO / epoch → Asia/Shanghai 墙钟；禁止透传原始 ms。 */
export function formatSloTimestamp(
  value: string | number | null | undefined,
): string | null {
  if (value == null || value === "") return null;
  const formatted = formatAlertTime(value);
  return formatted === "—" ? null : formatted;
}

/** 剩余预算展示为百分比文案；null → — */
export function formatBudgetRemaining(remaining: number | null | undefined): string {
  if (typeof remaining !== "number" || !Number.isFinite(remaining)) return "—";
  return `${(remaining * 100).toFixed(2)}%`;
}
