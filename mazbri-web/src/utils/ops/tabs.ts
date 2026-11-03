import { canAccessRoles, type RoleLike } from "./roles";

export interface OpsTabDef<T extends string = string> {
  id: T;
  /** 允许看见该 Tab 的角色；缺省或空视为全员。 */
  roles?: readonly string[];
  label?: string;
  disabled?: boolean;
}

export interface ResolveOpsTabOptions<T extends string> {
  role: RoleLike;
  tabs: readonly OpsTabDef<T>[];
  /** 路由 `?tab=` 原始值。 */
  queryTab?: unknown;
  /** 首选默认（若对当前角色不可见则回落到首个可见）。 */
  preferredDefault?: T;
}

export interface ResolveOpsTabResult<T extends string> {
  visibleTabs: OpsTabDef<T>[];
  activeTab: T | undefined;
  /** query 指向不可见/未知 tab，或缺失时回落到默认。 */
  fellBack: boolean;
}

function normalizeQueryTab(value: unknown): string | undefined {
  if (Array.isArray(value)) {
    const first = value[0];
    return typeof first === "string" ? first : undefined;
  }
  return typeof value === "string" ? value : undefined;
}

/** 按角色过滤 Tab，并解析当前应激活的 id（深链非法则回落）。 */
export function resolveOpsTab<T extends string>(
  options: ResolveOpsTabOptions<T>,
): ResolveOpsTabResult<T> {
  const visibleTabs = options.tabs.filter((tab) => {
    if (!tab.roles || tab.roles.length === 0) {
      return true;
    }
    return canAccessRoles(options.role, tab.roles);
  });

  if (visibleTabs.length === 0) {
    return { visibleTabs, activeTab: undefined, fellBack: true };
  }

  const queryId = normalizeQueryTab(options.queryTab);
  if (queryId && visibleTabs.some((tab) => tab.id === queryId)) {
    return {
      visibleTabs,
      activeTab: queryId as T,
      fellBack: false,
    };
  }

  const preferred = options.preferredDefault;
  if (preferred && visibleTabs.some((tab) => tab.id === preferred)) {
    return {
      visibleTabs,
      activeTab: preferred,
      fellBack: queryId !== undefined && queryId !== preferred,
    };
  }

  return {
    visibleTabs,
    activeTab: visibleTabs[0]?.id,
    fellBack: queryId !== undefined,
  };
}
