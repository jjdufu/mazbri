import type { SloDefinition, SloStatusRow } from "@/api/opsReliability";
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

export interface SloStyledCell {
  tag: "span";
  tone: "danger" | "muted";
  children: string;
}

export interface SloColumnOption {
  prop: keyof SloBudgetRow;
  label: string;
  width?: number;
  minWidth?: number;
  align?: "left" | "center" | "right";
  headerAlign?: "left" | "center" | "right";
  className?: string;
  formatter?: (row: SloBudgetRow) => string | SloStyledCell;
}

export const SLO_COLUMN_DEFS: readonly SloColumnDef[] = [
  { prop: "name", labelKey: "ops.slo.colName", minWidth: 180 },
  { prop: "sli_type", labelKey: "ops.slo.colType", width: 108 },
  { prop: "objective", labelKey: "ops.slo.colObjective", width: 104 },
  { prop: "window_days", labelKey: "ops.slo.colWindow", width: 96 },
  { prop: "sli", labelKey: "ops.slo.colSli", width: 104 },
  { prop: "remaining", labelKey: "ops.slo.colBudget", minWidth: 116 },
  { prop: "burn_5m", labelKey: "ops.slo.colBurn5m", minWidth: 104, adminOnly: true },
  { prop: "burn_page", labelKey: "ops.slo.colBurnPage", minWidth: 124, adminOnly: true },
  { prop: "burn_ticket", labelKey: "ops.slo.colBurnTicket", minWidth: 124, adminOnly: true },
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

/** 比率小数格式化为百分比（如 `0.999 -> "99.90%"`、`1 -> "100.00%"`）。 */
export function formatSloRatio(val: number | null | undefined): string {
  if (typeof val !== "number" || !Number.isFinite(val)) return "—";
  return `${(val * 100).toFixed(2)}%`;
}

/** 燃烧率倍率格式化（如 `0 -> "0x"`、`1.25 -> "1.25x"`）。 */
export function formatBurnRate(val: number | null | undefined): string {
  if (typeof val !== "number" || !Number.isFinite(val)) return "—";
  if (val === 0) return "0x";
  return `${parseFloat(val.toFixed(2))}x`;
}

function renderBurnFlag(
  val: number | null | undefined,
  t: SloTranslate,
): string | SloStyledCell {
  if (typeof val !== "number" || !Number.isFinite(val)) return "—";
  const firing = val > 0;
  const label = firing
    ? t("ops.slo.burnFiring")
    : t("ops.slo.burnNormal");
  return {
    tag: "span",
    tone: firing ? "danger" : "muted",
    children: label,
  };
}

export function buildSloTableColumns(
  t: SloTranslate,
  options: { adminDetail?: boolean } = {},
): SloColumnOption[] {
  const adminDetail = options.adminDetail ?? true;
  return SLO_COLUMN_DEFS.filter((d) => adminDetail || !d.adminOnly).map((d) => {
    const base: SloColumnOption = {
      prop: d.prop,
      label: t(d.labelKey),
      width: d.width,
      minWidth: d.minWidth,
    };
    switch (d.prop) {
      case "objective":
        return {
          ...base,
          align: "right",
          headerAlign: "right",
          className: "tabular-nums",
          formatter: (row) => formatSloRatio(row.objective),
        };
      case "sli":
        return {
          ...base,
          align: "right",
          headerAlign: "right",
          className: "tabular-nums",
          formatter: (row) => formatSloRatio(row.sli),
        };
      case "window_days":
        return {
          ...base,
          align: "right",
          headerAlign: "right",
          className: "tabular-nums",
        };
      case "remaining":
        return {
          ...base,
          align: "right",
          headerAlign: "right",
          className: "tabular-nums",
          formatter: (row) => {
            const text = formatBudgetRemaining(row.remaining);
            if (typeof row.remaining === "number" && row.remaining <= 0) {
              return {
                tag: "span",
                tone: "danger",
                children: text,
              };
            }
            return text;
          },
        };
      case "burn_5m":
        return {
          ...base,
          align: "right",
          headerAlign: "right",
          className: "tabular-nums",
          formatter: (row) => formatBurnRate(row.burn_5m),
        };
      case "burn_page":
        return {
          ...base,
          formatter: (row) => renderBurnFlag(row.burn_page, t),
        };
      case "burn_ticket":
        return {
          ...base,
          formatter: (row) => renderBurnFlag(row.burn_ticket, t),
        };
      default:
        return base;
    }
  });
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
