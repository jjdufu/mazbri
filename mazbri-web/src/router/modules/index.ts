import { AppRouteRecord } from "@/types/router";
import { exceptionRoutes } from "./exception";
import { mazbriRoutes } from "./mazbri";
import { controlPanelRoutes } from "./control-panel";
export const routeModules: AppRouteRecord[] = [
  ...mazbriRoutes,
  ...controlPanelRoutes,
  exceptionRoutes,
];
