import type { AppRouteRecord } from "@/types/router";
import { ROLES_ALL } from "@/utils/ops/roles";

/**
 * MFA 守卫返回的 `setup_path`（`/account/security`）与顶栏快捷入口。
 * 侧栏入口在控制面板「账号安全」；本路由保持 `isHide`，组件与控制面板子路由共用。
 */
export const accountRoutes: AppRouteRecord[] = [
  {
    path: "/account",
    name: "Account",
    component: "",
    meta: {
      title: "menus.mazbri.account",
      icon: "ri:user-line",
      isHide: true,
      roles: [...ROLES_ALL],
    },
    children: [
      {
        path: "security",
        name: "AccountSecurity",
        component: "/mazbri/account/security/index",
        meta: {
          title: "menus.mazbri.accountSecurity",
          icon: "ri:shield-user-line",
          isHide: true,
          keepAlive: false,
          roles: [...ROLES_ALL],
        },
      },
    ],
  },
];
