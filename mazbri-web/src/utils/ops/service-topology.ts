import type { ServiceHealthRow } from "@/types/api/monitoring";

/**
 * 静态架构拓扑能力表：仅此 2 个微服务不直连主业务 PostgreSQL 连接池。
 * 其余 9 个微服务（gateway / mgmt / cert / auth / billing / ratelimit / cache / cbpro-ext / bypass）
 * 均直连 PostgreSQL，严禁根据运行时指标是否为空动态隐藏其 DB 连接池卡片。
 */
export const SERVICES_WITHOUT_PG: ReadonlySet<string> = new Set([
  "mazbri-cbpro",
  "mazbri-analytics",
]);

export type HealthPanelMode = "apps" | "infra";

export function serviceHasPostgresPool(service: string): boolean {
  const normalized = service.trim();
  if (!normalized) return true;
  return !SERVICES_WITHOUT_PG.has(normalized);
}

export function serviceDetailStatColSpan(service: string): 6 | 8 {
  return serviceHasPostgresPool(service) ? 6 : 8;
}

export function filterRowsByGroup(
  rows: readonly ServiceHealthRow[],
  mode: HealthPanelMode,
): ServiceHealthRow[] {
  if (mode === "apps") {
    return rows.filter((row) => (row.group ?? "app") === "app");
  }
  return rows.filter((row) => (row.group ?? "app") !== "app");
}

export function formatCpuCoresDisplay(
  cores: number | null | undefined,
  coresUnitLabel: string,
): string {
  if (cores === null || cores === undefined || !Number.isFinite(cores)) {
    return "—";
  }
  if (cores === 0) return `0 ${coresUnitLabel}`;
  if (Math.abs(cores) < 0.1) {
    const mcpu = Number((cores * 1000).toPrecision(2));
    return `${mcpu} mCPU`;
  }
  return `${Number(cores.toFixed(2))} ${coresUnitLabel}`;
}
