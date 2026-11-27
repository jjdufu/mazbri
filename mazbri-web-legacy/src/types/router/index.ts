import { RouteRecordRaw } from "vue-router";
export interface RouteMeta extends Record<string | number | symbol, unknown> {
  title: string;
  icon?: string;
  showBadge?: boolean;
  showTextBadge?: string;
  isHide?: boolean;
  isHideTab?: boolean;
  link?: string;
  isIframe?: boolean;
  keepAlive?: boolean;
  authList?: Array<{
    title: string;
    authMark: string;
  }>;
  isFirstLevel?: boolean;
  roles?: string[];
  fixedTab?: boolean;
  activePath?: string;
  isFullPage?: boolean;
  isAuthButton?: boolean;
  authMark?: string;
  parentPath?: string;
}
export interface AppRouteRecord
  extends Omit<RouteRecordRaw, "meta" | "children" | "component"> {
  id?: number;
  meta: RouteMeta;
  children?: AppRouteRecord[];
  component?: string | (() => Promise<any>);
}
