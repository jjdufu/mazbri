import type { ChangeEvent } from "@/api/opsChanges";
import type { ColumnOption } from "@/types/component";

/** 变更表列定义（prop + i18n key + 宽度）——列表与抽屉共用一处。 */
export interface ChangeColumnDef {
  prop: keyof ChangeEvent | "detail";
  labelKey: string;
  width?: number;
  /** 仅抽屉展示（如 detail JSON）。 */
  drawerOnly?: boolean;
}

export const CHANGE_COLUMN_DEFS: readonly ChangeColumnDef[] = [
  { prop: "ts", labelKey: "ops.assets.colTime", width: 180 },
  { prop: "kind", labelKey: "ops.assets.colKind", width: 100 },
  { prop: "title", labelKey: "ops.assets.colTitle" },
  { prop: "actor", labelKey: "ops.assets.colActor", width: 120 },
  { prop: "resource_type", labelKey: "ops.assets.colResource", width: 120 },
  { prop: "service", labelKey: "ops.assets.colService", width: 100 },
  { prop: "status", labelKey: "ops.assets.colStatus", width: 90 },
  { prop: "detail", labelKey: "ops.overview.changeDetail", drawerOnly: true },
];

export type ChangeTranslate = (key: string) => string;

/** ArtTable 列表列（不含 drawerOnly）。 */
export function buildChangeTableColumns(
  t: ChangeTranslate,
): ColumnOption<ChangeEvent>[] {
  return CHANGE_COLUMN_DEFS.filter((d) => !d.drawerOnly).map((d) => ({
    prop: d.prop,
    label: t(d.labelKey),
    width: d.width,
  }));
}

/** 抽屉字段顺序（含 detail）。 */
export function buildChangeDrawerFields(
  t: ChangeTranslate,
): { prop: ChangeColumnDef["prop"]; label: string }[] {
  return CHANGE_COLUMN_DEFS.map((d) => ({
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
export function normalizeChangeEvents(
  rows: readonly Partial<ChangeEvent>[] | null | undefined,
): ChangeEvent[] {
  if (!rows?.length) return [];
  return rows.map((row) => ({
    ts: String(row.ts ?? ""),
    kind: String(row.kind ?? ""),
    title: String(row.title ?? ""),
    actor: row.actor ?? null,
    resource_type: row.resource_type ?? null,
    resource_id: row.resource_id ?? null,
    status: row.status ?? null,
    service: row.service ?? null,
    detail: row.detail ? asRecord(row.detail) : undefined,
  }));
}

/** 截取近 N 条（输入假定已按时间倒序）。 */
export function takeChangeTop(
  rows: readonly ChangeEvent[],
  limit = 20,
): ChangeEvent[] {
  const n = Math.max(0, Math.floor(limit));
  if (n === 0) return [];
  return rows.slice(0, n);
}

/** 近 24h Unix 秒窗（end 对齐到秒）。 */
export function last24hWindow(nowMs: number = Date.now()): {
  start: number;
  end: number;
} {
  const end = Math.floor(nowMs / 1000);
  return { start: end - 86400, end };
}

/** 注解颜色：route / publish / 其它。 */
export function changeAnnotationColor(kind: string): string {
  if (kind === "route") return "#409eff";
  if (kind === "publish") return "#67c23a";
  return "#e6a23c";
}

export interface ChangeChartAnnotation {
  timestamp: number;
  label: string;
  color: string;
}

/** 从变更事件生成图表注解（无效时间戳跳过）。 */
export function buildChangeAnnotations(
  events: readonly ChangeEvent[],
): ChangeChartAnnotation[] {
  const out: ChangeChartAnnotation[] = [];
  for (const event of events) {
    const ms = Date.parse(event.ts);
    if (!Number.isFinite(ms)) continue;
    out.push({
      timestamp: Math.floor(ms / 1000),
      label: event.kind || "change",
      color: changeAnnotationColor(event.kind),
    });
  }
  return out;
}

/** 抽屉/复制用的 detail 文本。 */
export function formatChangeDetail(
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
export function changeFieldDisplay(
  row: ChangeEvent,
  prop: ChangeColumnDef["prop"],
): string {
  if (prop === "detail") return formatChangeDetail(row.detail);
  const value = row[prop];
  if (value === null || value === undefined || value === "") return "—";
  return String(value);
}
