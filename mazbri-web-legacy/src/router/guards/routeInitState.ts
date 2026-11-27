import type { Router } from "vue-router";
import type { LocationQuery } from "vue-router";
import { RouteRegistry } from "../core";
export interface DeferredNavigationTarget {
  path: string;
  query: LocationQuery;
  hash: string;
}
export class RouteInitStateOwner {
  private routeRegistry: RouteRegistry | null = null;
  private pendingLoading = false;
  private routeInitFailed = false;
  private routeInitInProgress = false;
  private deferredNavigation: DeferredNavigationTarget | null = null;
  initializeRouteRegistry(router: Router): RouteRegistry {
    const registry = new RouteRegistry(router);
    this.routeRegistry = registry;
    return registry;
  }
  getRouteRegistry(): RouteRegistry | null {
    return this.routeRegistry;
  }
  getPendingLoading(): boolean {
    return this.pendingLoading;
  }
  setPendingLoading(value: boolean): void {
    this.pendingLoading = value;
  }
  resetPendingLoading(): void {
    this.pendingLoading = false;
  }
  getRouteInitFailed(): boolean {
    return this.routeInitFailed;
  }
  markRouteInitFailed(): void {
    this.routeInitFailed = true;
  }
  getRouteInitInProgress(): boolean {
    return this.routeInitInProgress;
  }
  beginRouteInit(): void {
    this.routeInitInProgress = true;
  }
  finishRouteInit(): void {
    this.routeInitInProgress = false;
  }
  setDeferredNavigation(target: DeferredNavigationTarget): void {
    this.deferredNavigation = target;
  }
  consumeDeferredNavigation(): DeferredNavigationTarget | null {
    const target = this.deferredNavigation;
    this.deferredNavigation = null;
    return target;
  }
  clearDeferredNavigation(): void {
    this.deferredNavigation = null;
  }
  reset(): void {
    this.pendingLoading = false;
    this.routeInitFailed = false;
    this.routeInitInProgress = false;
    this.deferredNavigation = null;
  }
}
export const routeInitState = new RouteInitStateOwner();
