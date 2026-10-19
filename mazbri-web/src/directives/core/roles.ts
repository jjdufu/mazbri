import { useUserStore } from "@/store/modules/user";
import type { AdminRole } from "@/types/api";
import { App, Directive, DirectiveBinding } from "vue";
interface RolesBinding extends DirectiveBinding {
  value: string | string[];
}
function checkRolePermission(el: HTMLElement, binding: RolesBinding): void {
  const userStore = useUserStore();
  const userRoles = userStore.getUserInfo.roles;
  if (!userRoles?.length) {
    removeElement(el);
    return;
  }
  const requiredRoles = Array.isArray(binding.value)
    ? binding.value
    : [binding.value];
  const hasPermission = requiredRoles.some((role: string) =>
    userRoles.includes(role as AdminRole),
  );
  if (!hasPermission) {
    removeElement(el);
  }
}
function removeElement(el: HTMLElement): void {
  if (el.parentNode) {
    el.parentNode.removeChild(el);
  }
}
const rolesDirective: Directive = {
  mounted: checkRolePermission,
  updated: checkRolePermission,
};
export function setupRolesDirective(app: App): void {
  app.directive("roles", rolesDirective);
}
