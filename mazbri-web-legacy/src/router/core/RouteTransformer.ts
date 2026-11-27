import type { RouteRecordRaw } from "vue-router";
import type { AppRouteRecord } from "@/types/router";
import { ComponentLoader } from "./ComponentLoader";

interface ConvertedRoute extends Omit<RouteRecordRaw, "children"> {
  id?: number;
  children?: ConvertedRoute[];
  component?: RouteRecordRaw["component"] | (() => Promise<any>);
}

export class RouteTransformer {
  private componentLoader: ComponentLoader;

  constructor(componentLoader: ComponentLoader) {
    this.componentLoader = componentLoader;
  }

  transform(route: AppRouteRecord, depth = 0): ConvertedRoute {
    const { component, children, ...routeConfig } = route;
    const converted: ConvertedRoute = {
      ...routeConfig,
      component: undefined,
    };
    if (depth === 0) {
      this.handleTopLevelRoute(converted, route, component as string);
    } else {
      this.handleNormalRoute(converted, component as string);
    }
    if (children?.length) {
      converted.children = children.map((child) =>
        this.transform(child, depth + 1),
      );
    }
    return converted;
  }

  private handleTopLevelRoute(
    converted: ConvertedRoute,
    route: AppRouteRecord,
    component: string | undefined,
  ): void {
    converted.component = this.componentLoader.loadLayout();
    route.meta.isFirstLevel = true;
    if (component && !route.children?.length) {
      converted.path = this.extractFirstSegment(route.path || "");
      converted.name = "";
      converted.children = [
        {
          ...route,
          component: this.componentLoader.load(component),
        } as ConvertedRoute,
      ];
    }
  }

  private handleNormalRoute(
    converted: ConvertedRoute,
    component: string | undefined,
  ): void {
    if (component) {
      converted.component = this.componentLoader.load(component);
    }
  }

  private extractFirstSegment(path: string): string {
    const segments = path.split("/").filter(Boolean);
    return segments.length > 0 ? `/${segments[0]}` : "/";
  }
}
