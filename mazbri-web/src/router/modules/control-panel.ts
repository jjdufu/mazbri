import type { AppRouteRecord } from "@/types/router";
export const controlPanelRoutes: AppRouteRecord[] = [
  {
    path: "/control-panel",
    name: "ControlPanel",
    component: "",
    meta: {
      title: "menus.mazbri.controlPanel",
      icon: "ri:admin-line",
      order: 2,
      roles: ["R_ADMIN"],
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
          roles: ["R_ADMIN"],
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
          keepAlive: true,
          roles: ["R_ADMIN"],
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
          roles: ["R_ADMIN"],
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
          keepAlive: true,
          roles: ["R_ADMIN"],
        },
      },
      {
        path: "certificate-presets",
        name: "CertificatePresets",
        component: "/mazbri/control-panel/certificates/presets/index",
        meta: {
          title: "menus.mazbri.certificatePresets",
          icon: "ri:file-copy-line",
          keepAlive: true,
          roles: ["R_ADMIN"],
        },
      },
      {
        path: "authorizations",
        name: "AuthorizationManagement",
        component: "/mazbri/control-panel/authorizations/index",
        meta: {
          title: "menus.mazbri.authorizationManagement",
          icon: "ri:shield-keyhole-line",
          keepAlive: true,
          roles: ["R_ADMIN"],
        },
      },
      {
        path: "password-policy",
        name: "PasswordPolicy",
        component: "/mazbri/control-panel/security/password-policy/index",
        meta: {
          title: "menus.mazbri.passwordPolicy",
          icon: "ri:lock-password-line",
          keepAlive: true,
          roles: ["R_ADMIN"],
        },
      },
      {
        path: "system",
        name: "SystemConfig",
        component: "/mazbri/control-panel/system/index",
        meta: {
          title: "menus.mazbri.systemSettings",
          icon: "ri:settings-4-line",
          keepAlive: true,
          roles: ["R_ADMIN"],
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
          roles: ["R_ADMIN"],
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
          roles: ["R_ADMIN"],
        },
      },
    ],
  },
];
