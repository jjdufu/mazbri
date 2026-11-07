import type {
  ComponentGroup,
  ComponentHealth,
  ComponentStatus,
} from "@/types/api/monitoring";

/** 展示分组顺序（与后端 ComponentGroup 声明顺序一致）。 */
export const COMPONENT_GROUP_ORDER: readonly ComponentGroup[] = [
  "app",
  "middleware",
  "observability",
  "other",
];

export interface ComponentGroupView {
  group: ComponentGroup;
  items: ComponentHealth[];
  abnormal: number;
}

const STATUS_RANK: Record<ComponentStatus, number> = {
  red: 0,
  yellow: 1,
  unknown: 2,
  green: 3,
};

export function isAbnormal(status: ComponentStatus): boolean {
  return status !== "green";
}

/**
 * 按分组聚合；组内异常优先（red → yellow → unknown → green），同级按名称；
 * 按 key 去重（后端已去重，这里兜底防止重复渲染）。
 */
export function groupComponents(
  components: readonly ComponentHealth[] | null | undefined,
): ComponentGroupView[] {
  const seen = new Set<string>();
  const byGroup = new Map<ComponentGroup, ComponentHealth[]>();
  for (const c of components ?? []) {
    if (seen.has(c.key)) continue;
    seen.add(c.key);
    const list = byGroup.get(c.group) ?? [];
    list.push(c);
    byGroup.set(c.group, list);
  }
  const out: ComponentGroupView[] = [];
  for (const group of COMPONENT_GROUP_ORDER) {
    const items = byGroup.get(group);
    if (!items?.length) continue;
    items.sort(
      (a, b) =>
        STATUS_RANK[a.status] - STATUS_RANK[b.status] ||
        a.name.localeCompare(b.name, "zh-CN"),
    );
    out.push({ group, items, abnormal: items.filter((c) => isAbnormal(c.status)).length });
  }
  return out;
}

export function countAbnormal(
  components: readonly ComponentHealth[] | null | undefined,
): number {
  return groupComponents(components).reduce((n, g) => n + g.abnormal, 0);
}

/**
 * 提取构建版本号的主版本段（剥离括号内的 git commit SHA），用于六边形等紧凑角标展示。
 * 例：`"0.3.5 (98ac85f38ca0)"` -> `"v0.3.5"`。
 */
export function formatShortVersion(
  version: string | null | undefined,
): string | undefined {
  if (!version) return undefined;
  const base = version.replace(/\s*\([^)]*\).*$/, "").trim();
  if (!base) return undefined;
  return base.startsWith("v") ? base : `v${base}`;
}
