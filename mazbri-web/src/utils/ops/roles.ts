import type { AdminRole } from "@/types/api";

/** 与后端 `route_access::{ADMIN, BIZ, ALL_ROLES}` 对齐的角色集合常量。 */
export const ROLE_ADMIN: AdminRole = "R_ADMIN";
export const ROLE_FINANCE: AdminRole = "R_FINANCE";
export const ROLE_VIEWER: AdminRole = "R_VIEWER";

/** 超管：仅 R_ADMIN。 */
export const ROLES_ADMIN: readonly AdminRole[] = [ROLE_ADMIN];

/** 业务读写（BIZ）：R_ADMIN + R_FINANCE。 */
export const ROLES_BIZ: readonly AdminRole[] = [ROLE_ADMIN, ROLE_FINANCE];

/** 全部已知角色。 */
export const ROLES_ALL: readonly AdminRole[] = [
  ROLE_ADMIN,
  ROLE_FINANCE,
  ROLE_VIEWER,
];

export type RoleLike = string | null | undefined;

/** 规范化当前角色；未知值返回 undefined。 */
export function normalizeAdminRole(role: RoleLike): AdminRole | undefined {
  if (role === ROLE_ADMIN || role === ROLE_FINANCE || role === ROLE_VIEWER) {
    return role;
  }
  return undefined;
}

export function isAdminRole(role: RoleLike): boolean {
  return normalizeAdminRole(role) === ROLE_ADMIN;
}

/** 与后端 BIZ 一致：超管或管理员。 */
export function isBizRole(role: RoleLike): boolean {
  const normalized = normalizeAdminRole(role);
  return normalized === ROLE_ADMIN || normalized === ROLE_FINANCE;
}

export function isViewerRole(role: RoleLike): boolean {
  return normalizeAdminRole(role) === ROLE_VIEWER;
}

/**
 * 上游免代码工作台写权限：与后端 `BIZ` 一致（`R_ADMIN` + `R_FINANCE`）。
 * `R_VIEWER` 仅只读浏览。
 */
export function canWriteUpstreamConnector(role: RoleLike): boolean {
  return isBizRole(role);
}

/**
 * 当前角色是否命中允许列表。
 * - `allowed` 为空视为拒绝（与后端 Roles([]) 非法一致，防御性）。
 * - 未知角色一律拒绝。
 */
export function canAccessRoles(
  role: RoleLike,
  allowed: readonly string[] | undefined | null,
): boolean {
  if (!allowed || allowed.length === 0) {
    return false;
  }
  const normalized = normalizeAdminRole(role);
  if (!normalized) {
    return false;
  }
  return allowed.includes(normalized);
}

/** 过滤带 `roles` 字段的条目；无 `roles` 字段视为全员可见。 */
export function filterByRoles<T extends { roles?: readonly string[] }>(
  role: RoleLike,
  items: readonly T[],
): T[] {
  return items.filter((item) => {
    if (!item.roles || item.roles.length === 0) {
      return true;
    }
    return canAccessRoles(role, item.roles);
  });
}
