import type { AppRouteRecord } from "@/types/router";
import { logger } from "@/utils/sys/logger";
import { ErrorCodes } from "@/constants/error-codes";
export class IframeRouteManager {
  private static instance: IframeRouteManager;
  private iframeRoutes: AppRouteRecord[] = [];
  private constructor() {}
  static getInstance(): IframeRouteManager {
    if (!IframeRouteManager.instance) {
      IframeRouteManager.instance = new IframeRouteManager();
    }
    return IframeRouteManager.instance;
  }
  add(route: AppRouteRecord): void {
    if (!this.iframeRoutes.find((r) => r.path === route.path)) {
      this.iframeRoutes.push(route);
    }
  }
  getAll(): AppRouteRecord[] {
    return this.iframeRoutes;
  }
  findByPath(path: string): AppRouteRecord | undefined {
    return this.iframeRoutes.find((route) => route.path === path);
  }
  clear(): void {
    this.iframeRoutes = [];
  }
  save(): void {
    if (this.iframeRoutes.length > 0) {
      sessionStorage.setItem("iframeRoutes", JSON.stringify(this.iframeRoutes));
    }
  }
  load(): void {
    try {
      const data = sessionStorage.getItem("iframeRoutes");
      if (data) {
        this.iframeRoutes = JSON.parse(data);
      }
    } catch (error) {
      logger.error("[IframeRouteManager] Failed to load iframe routes", {
        service: "mazbri-mgmt-web",
        event: "iframe_route_load_failure",
        severity: "error",
        error_code: ErrorCodes.INTERNAL_ERROR,
        error,
      });
      this.iframeRoutes = [];
    }
  }
}
