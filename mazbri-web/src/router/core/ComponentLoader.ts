import { h } from "vue";
import { $t } from "@/locales";
import { logger } from "@/utils/sys";
export class ComponentLoader {
  private modules: Record<string, () => Promise<any>>;
  constructor() {
    this.modules = import.meta.glob("../../views/**/*.vue");
  }
  load(componentPath: string): () => Promise<any> {
    if (!componentPath) {
      return this.createEmptyComponent();
    }
    const fullPath = `../../views${componentPath}.vue`;
    const fullPathWithIndex = `../../views${componentPath}/index.vue`;
    const module = this.modules[fullPath] || this.modules[fullPathWithIndex];
    if (!module) {
      logger.error(
        `[ComponentLoader] Component not found: ${componentPath}, tried: ${fullPath} and ${fullPathWithIndex}`,
      );
      return this.createErrorComponent(componentPath);
    }
    return module;
  }
  loadLayout(): () => Promise<any> {
    return () => import("@/views/index/index.vue");
  }
  loadIframe(): () => Promise<any> {
    return () => import("@/views/outside/Iframe.vue");
  }
  private createEmptyComponent(): () => Promise<any> {
    return () =>
      Promise.resolve({
        render() {
          return h("div", {});
        },
      });
  }
  private createErrorComponent(componentPath: string): () => Promise<any> {
    return () =>
      Promise.resolve({
        render() {
          return h(
            "div",
            { class: "route-error" },
            $t("common.errors.componentNotFound", { componentPath }),
          );
        },
      });
  }
}
