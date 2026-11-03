import { computed } from "vue";
import { storeToRefs } from "pinia";
import { useUserStore } from "@/store/modules/user";
import type { AdminRole } from "@/types/api";
import {
  canAccessRoles,
  filterByRoles,
  isAdminRole,
  isBizRole,
  isViewerRole,
  normalizeAdminRole,
  ROLES_ADMIN,
  ROLES_ALL,
  ROLES_BIZ,
} from "@/utils/ops/roles";

/**
 * 运维 / Dashboard / 账号安全共用的角色门控。
 * 基于 `userStore.info.role`，与后端 route_access 的 ADMIN / BIZ / ALL 对齐。
 */
export function useOpsRole() {
  const userStore = useUserStore();
  const { info, isAdmin: storeIsAdmin, canWriteBusiness } = storeToRefs(userStore);

  const role = computed<AdminRole | undefined>(() =>
    normalizeAdminRole(info.value?.role),
  );

  const isAdmin = computed(() => isAdminRole(role.value) || storeIsAdmin.value);
  const isBiz = computed(
    () => isBizRole(role.value) || canWriteBusiness.value,
  );
  const isViewer = computed(() => isViewerRole(role.value));

  function canAccess(allowed: readonly string[] | undefined | null): boolean {
    return canAccessRoles(role.value, allowed);
  }

  function filterItems<T extends { roles?: readonly string[] }>(
    items: readonly T[],
  ): T[] {
    return filterByRoles(role.value, items);
  }

  return {
    role,
    isAdmin,
    isBiz,
    isViewer,
    canAccess,
    canAccessRoles: canAccess,
    filterByRoles: filterItems,
    ROLES_ADMIN,
    ROLES_BIZ,
    ROLES_ALL,
  };
}

export {
  canAccessRoles,
  filterByRoles,
  isAdminRole,
  isBizRole,
  isViewerRole,
  normalizeAdminRole,
  ROLES_ADMIN,
  ROLES_ALL,
  ROLES_BIZ,
};
