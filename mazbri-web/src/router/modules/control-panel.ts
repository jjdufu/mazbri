import type { AppRouteRecord } from "@/types/router";
import { ROLES_ADMIN, ROLES_ALL } from "@/utils/ops/roles";

/**
 * 控制面板。
 * - 父级对全部角色可见，以便 Finance/Viewer 能看到「账号安全」。
 * - 其余管理子项仍仅 R_ADMIN；侧栏按子项 roles 过滤，非超管不会看到用户/证书等。
 * - 「账号安全」与隐藏路由 `/account/security` 共用同一页面组件（后者是 MFA setup_path）。
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
      roles: [...ROLES_ALL],
    },
    children: [
      {
        path: "account-security",
        name: "ControlPanelAccountSecurity",
        component: "/mazbri/account/security/index",
        meta: {
          title: "menus.mazbri.accountSecurity",
          icon: "ri:shield-user-line",
          keepAlive: false,
          roles: [...ROLES_ALL],
        },
      },
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
          title: "menus.mazbri.certificateList",
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
        path: "authorizations",
        name: "AuthorizationManagement",
        component: "/mazbri/control-panel/authorizations/index",
        meta: {
          title: "menus.mazbri.authorizationManagement",
          icon: "ri:shield-keyhole-line",
          keepAlive: false,
          roles: [...ROLES_ADMIN],
        },
      },
      {
        path: "system",
        name: "SystemConfig",
        component: "/mazbri/control-panel/system/index",
        meta: {
          title: "menus.mazbri.systemSettings",
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
