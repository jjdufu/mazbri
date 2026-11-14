import type { AppRouteRecord } from "@/types/router";
import { ROLES_ADMIN } from "@/utils/ops/roles";

/**
 * 控制面板（仅对超级管理员 R_ADMIN 开放；个人账号与安全统一由顶栏 PersonalSecurityDrawer 承载）。
 */
export const controlPanelRoutes: AppRouteRecord[] = [
  {
    path: "/control-panel",
    name: "ControlPanel",
    component: "",
    meta: {
      title: "menus.mazbri.controlPanel",
      icon: "ri:admin-line",
      order: 30,
      roles: [...ROLES_ADMIN],
    },
    children: [
      {
        path: "users",
        name: "UserList",
        component: "/mazbri/control-panel/users/index",
        meta: {
          title: "menus.mazbri.userManagement",
          icon: "ri:user-settings-line",
          keepAlive: false,
          roles: [...ROLES_ADMIN],
        },
      },
      {
        path: "user/:id",
        name: "UserDetail",
        component: "/mazbri/control-panel/users/[id]/index",
        meta: {
          title: "menus.mazbri.userDetail",
          icon: "ri:user-line",
          isHide: true,
          activePath: "/control-panel/users",
          keepAlive: false,
          roles: [...ROLES_ADMIN],
        },
      },
      {
        path: "certificates",
        name: "CertificateList",
        component: "/mazbri/control-panel/certificates/index",
        meta: {
          title: "menus.mazbri.certificates",
          icon: "ri:award-line",
          keepAlive: false,
          roles: [...ROLES_ADMIN],
        },
      },
      {
        path: "certificate/:id",
        name: "CertificateDetail",
        component: "/mazbri/control-panel/certificates/[id]/index",
        meta: {
          title: "menus.mazbri.certificateDetail",
          icon: "ri:award-line",
          isHide: true,
          activePath: "/control-panel/certificates",
          keepAlive: false,
          roles: [...ROLES_ADMIN],
        },
      },
      {
        path: "system",
        name: "SystemConfig",
        component: "/mazbri/control-panel/system/index",
        meta: {
          title: "menus.mazbri.platformSettings",
          icon: "ri:settings-4-line",
          keepAlive: false,
          roles: [...ROLES_ADMIN],
        },
      },
      {
        path: "audit-logs",
        name: "AuditLogs",
        component: "/mazbri/control-panel/audit-logs/index",
        meta: {
          title: "menus.mazbri.auditLogs",
          icon: "ri:file-history-line",
          keepAlive: false,
          roles: [...ROLES_ADMIN],
        },
      },
      {
        path: "config-center",
        name: "ConfigCenter",
        component: "/mazbri/control-panel/config-center/index",
        meta: {
          title: "menus.mazbri.configCenter",
          icon: "ri:settings-3-line",
          keepAlive: false,
          roles: [...ROLES_ADMIN],
        },
      },
    ],
  },
];
