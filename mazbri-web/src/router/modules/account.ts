import type { AppRouteRecord } from "@/types/router";
/**
 * 当前登录账号的自助页面（不在侧边菜单中展示，从右上角用户菜单进入）。
 * `/account/security` 同时是后端 MFA 守卫返回 `MFA_NOT_ENABLED` 时的 `setup_path`。
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
      roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
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
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
        },
      },
    ],
  },
];
