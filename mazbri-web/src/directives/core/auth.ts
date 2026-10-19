import { router } from "@/router";
import { App, Directive, DirectiveBinding } from "vue";
interface AuthBinding extends DirectiveBinding {
  value: string;
}
function checkAuthPermission(el: HTMLElement, binding: AuthBinding): void {
  const authList =
    (router.currentRoute.value.meta.authList as Array<{
      authMark: string;
    }>) || [];
  const hasPermission = authList.some(
    (item) => item.authMark === binding.value,
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
const authDirective: Directive = {
  mounted: checkAuthPermission,
  updated: checkAuthPermission,
};
export function setupAuthDirective(app: App): void {
  app.directive("auth", authDirective);
}
