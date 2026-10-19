import type { RouteRecordRaw } from "vue-router";
import type { AppRouteRecord } from "@/types/router";
import { ComponentLoader } from "./ComponentLoader";
import { IframeRouteManager } from "./IframeRouteManager";
interface ConvertedRoute extends Omit<RouteRecordRaw, "children"> {
  id?: number;
  children?: ConvertedRoute[];
  component?: RouteRecordRaw["component"] | (() => Promise<any>);
}
export class RouteTransformer {
  private componentLoader: ComponentLoader;
  private iframeManager: IframeRouteManager;
  constructor(componentLoader: ComponentLoader) {
    this.componentLoader = componentLoader;
    this.iframeManager = IframeRouteManager.getInstance();
  }
  transform(route: AppRouteRecord, depth = 0): ConvertedRoute {
    const { component, children, ...routeConfig } = route;
    const converted: ConvertedRoute = {
      ...routeConfig,
      component: undefined,
    };
    if (route.meta.isIframe) {
      this.handleIframeRoute(converted, route, depth);
    } else if (depth === 0) {
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
  private handleIframeRoute(
    targetRoute: ConvertedRoute,
    sourceRoute: AppRouteRecord,
    depth: number,
  ): void {
    if (depth === 0) {
      targetRoute.component = this.componentLoader.loadLayout();
      targetRoute.path = this.extractFirstSegment(sourceRoute.path || "");
      targetRoute.name = "";
      targetRoute.children = [
        {
          ...sourceRoute,
          component: this.componentLoader.loadIframe(),
        } as ConvertedRoute,
      ];
    } else {
      targetRoute.component = this.componentLoader.loadIframe();
    }
    this.iframeManager.add(sourceRoute);
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
