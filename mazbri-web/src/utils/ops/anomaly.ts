import type { AnomalyEvent } from "@/api/opsReliability";
import type { ColumnOption } from "@/types/component";

/** 异常表列定义（prop + i18n key + 宽度）——列表与抽屉共用一处。 */
export interface AnomalyColumnDef {
  prop: keyof AnomalyEvent | "detail";
  labelKey: string;
  width?: number;
  /** 仅抽屉展示（如 detail JSON）。 */
  drawerOnly?: boolean;
}

export const ANOMALY_COLUMN_DEFS: readonly AnomalyColumnDef[] = [
  { prop: "detected_at", labelKey: "ops.anomaly.colTime", width: 180 },
  { prop: "kind", labelKey: "ops.anomaly.colKind", width: 120 },
  { prop: "severity", labelKey: "ops.anomaly.colSev", width: 90 },
  { prop: "entity_id", labelKey: "ops.anomaly.colEntity", width: 140 },
  { prop: "metric", labelKey: "ops.anomaly.colMetric", width: 90 },
  { prop: "value", labelKey: "ops.anomaly.colValue", width: 100 },
  { prop: "baseline", labelKey: "ops.anomaly.colBase", width: 100 },
  { prop: "score", labelKey: "ops.anomaly.colScore", width: 90 },
  { prop: "status", labelKey: "ops.anomaly.colStatus", width: 90 },
  { prop: "detail", labelKey: "ops.anomaly.colDetail", drawerOnly: true },
];

export type AnomalyTranslate = (key: string) => string;

/** ArtTable 列表列（不含 drawerOnly）。 */
export function buildAnomalyTableColumns(
  t: AnomalyTranslate,
): ColumnOption<AnomalyEvent>[] {
  return ANOMALY_COLUMN_DEFS.filter((d) => !d.drawerOnly).map((d) => ({
    prop: d.prop,
    label: t(d.labelKey),
    width: d.width,
  }));
}

/** 抽屉字段顺序（含 detail）。 */
export function buildAnomalyDrawerFields(
  t: AnomalyTranslate,
): { prop: AnomalyColumnDef["prop"]; label: string }[] {
  return ANOMALY_COLUMN_DEFS.map((d) => ({
    prop: d.prop,
    label: t(d.labelKey),
  }));
}

function asRecord(value: unknown): Record<string, unknown> {
  if (value && typeof value === "object" && !Array.isArray(value)) {
    return value as Record<string, unknown>;
  }
  return {};
}

/** 归一化 API 行；缺字段给安全默认。 */
export function normalizeAnomalyEvents(
  rows: readonly Partial<AnomalyEvent>[] | null | undefined,
): AnomalyEvent[] {
  if (!rows?.length) return [];
  return rows.map((row) => ({
    id: String(row.id ?? ""),
    detected_at: String(row.detected_at ?? ""),
    kind: String(row.kind ?? ""),
    severity: String(row.severity ?? ""),
    entity_type: String(row.entity_type ?? ""),
    entity_id: String(row.entity_id ?? ""),
    metric: String(row.metric ?? ""),
    value: Number(row.value ?? 0),
    baseline: Number(row.baseline ?? 0),
    score: Number(row.score ?? 0),
    detail: asRecord(row.detail),
    status: String(row.status ?? ""),
  }));
}

/** 截取 Top N（输入假定已按时间倒序）。 */
export function takeAnomalyTop(
  rows: readonly AnomalyEvent[],
  limit = 10,
): AnomalyEvent[] {
  const n = Math.max(0, Math.floor(limit));
  if (n === 0) return [];
  return rows.slice(0, n);
}

/** 抽屉/复制用的 detail 文本。 */
export function formatAnomalyDetail(
  detail: Record<string, unknown> | null | undefined,
): string {
  if (!detail || Object.keys(detail).length === 0) return "—";
  try {
    return JSON.stringify(detail, null, 2);
  } catch {
    return String(detail);
  }
}

/** 按列 prop 取展示值。 */
export function anomalyFieldDisplay(
  row: AnomalyEvent,
  prop: AnomalyColumnDef["prop"],
): string {
  if (prop === "detail") return formatAnomalyDetail(row.detail);
  const value = row[prop];
  if (value === null || value === undefined || value === "") return "—";
  if (typeof value === "number") {
    return Number.isFinite(value) ? String(value) : "—";
  }
  return String(value);
}
