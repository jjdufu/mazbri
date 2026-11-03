import type { AppRouteRecord } from "@/types/router";
export const mazbriRoutes: AppRouteRecord[] = [
  {
    path: "/dashboard",
    name: "MazbriDashboard",
    component: "/mazbri/ops/overview/index",
    meta: {
      title: "menus.mazbri.opsOverview",
      icon: "ri:dashboard-line",
      order: 1,
      isHide: true,
      keepAlive: false,
      roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
    },
  },
  {
    path: "/screen",
    name: "OpsScreen",
    component: "/mazbri/ops/screen/index",
    meta: {
      title: "menus.mazbri.opsScreen",
      icon: "ri:tv-2-line",
      order: 0,
      isHide: true,
      isHideTab: true,
      isFullPage: true,
      keepAlive: false,
      roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
    },
  },
  {
    path: "/finance/overview",
    name: "FinanceOverview",
    component: "/mazbri/ops/finance/index",
    meta: {
      title: "menus.mazbri.financeOverview",
      icon: "ri:line-chart-line",
      order: 2,
      isHide: true,
      keepAlive: false,
      roles: ["R_FINANCE"],
    },
  },
  {
    path: "/operations",
    name: "OpsManagement",
    component: "",
    meta: {
      title: "menus.mazbri.opsManagement",
      icon: "ri:building-2-line",
      order: 10,
      roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
    },
    children: [
      {
        path: "tenants",
        name: "TenantList",
        component: "/mazbri/tenants/index",
        meta: {
          title: "menus.mazbri.tenantList",
          icon: "ri:building-line",
          keepAlive: false,
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
        },
      },
      {
        path: "tenants/:code",
        name: "TenantDetail",
        component: "/mazbri/tenants/[code]/index",
        meta: {
          title: "menus.mazbri.tenantDetail",
          icon: "ri:building-line",
          isHide: true,
          keepAlive: true,
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
        },
      },
      {
        path: "product-authorization",
        name: "ProductAuthorization",
        component: "/mazbri/product-authorization/index",
        meta: {
          title: "menus.mazbri.productAuthorization",
          icon: "ri:shield-user-line",
          keepAlive: false,
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
        },
      },
      {
        path: "products/:id",
        name: "ProductDetail",
        component: "/mazbri/products/[id]/index",
        meta: {
          title: "menus.mazbri.productDetail",
          icon: "ri:apps-line",
          isHide: true,
          activePath: "/operations/product-authorization",
          keepAlive: true,
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
        },
      },
      {
        path: "upstream-supply",
        name: "UpstreamSupply",
        component: "/mazbri/upstream-supply/index",
        meta: {
          title: "menus.mazbri.upstreamSupply",
          icon: "ri:building-4-line",
          keepAlive: false,
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
        },
      },
      {
        path: "upstream-supply/workbench",
        name: "UpstreamConnectorWorkbench",
        component: "/mazbri/upstream-connectors/workbench",
        meta: {
          title: "upstreamConnector.workbenchTitle",
          icon: "ri:plug-line",
          isHide: true,
          activePath: "/operations/upstream-supply",
          keepAlive: false,
          roles: ["R_ADMIN", "R_FINANCE"],
        },
      },
      {
        path: "upstream-supply/suppliers/:id",
        name: "SupplierDetail",
        component: "/mazbri/suppliers/[id]/index",
        meta: {
          title: "supplier.detail.pageTitle",
          icon: "ri:building-4-line",
          isHide: true,
          activePath: "/operations/upstream-supply",
          keepAlive: false,
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
        },
      },
      {
        path: "upstream-supply/suppliers/:id/edit",
        name: "SupplierEdit",
        component: "/mazbri/suppliers/[id]/edit",
        meta: {
          title: "supplier.form.editTitle",
          icon: "ri:building-4-line",
          isHide: true,
          activePath: "/operations/upstream-supply",
          keepAlive: false,
          roles: ["R_ADMIN", "R_FINANCE"],
        },
      },
      {
        path: "upstream-supply/channels/:id",
        name: "ChannelDetail",
        component: "/mazbri/channels/[id]/index",
        meta: {
          title: "channels.detail.pageTitle",
          icon: "ri:building-4-line",
          isHide: true,
          activePath: "/operations/upstream-supply",
          keepAlive: false,
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
        },
      },
      {
        path: "upstream-supply/channels/:id/edit",
        name: "ChannelEdit",
        component: "/mazbri/channels/[id]/edit",
        meta: {
          title: "channels.dialog.editTitle",
          icon: "ri:building-4-line",
          isHide: true,
          activePath: "/operations/upstream-supply",
          keepAlive: false,
          roles: ["R_ADMIN", "R_FINANCE"],
        },
      },
      {
        path: "callbacks",
        name: "CallbackMonitor",
        component: "/mazbri/operations/callbacks/index",
        meta: {
          title: "menus.mazbri.callbackList",
          icon: "ri:webhook-line",
          keepAlive: false,
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
        },
      },
      {
        path: "bills",
        name: "BillList",
        component: "/mazbri/billing/bills/index",
        meta: {
          title: "menus.mazbri.billList",
          icon: "ri:file-list-3-line",
          keepAlive: false,
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
        },
      },
      {
        path: "usage-reconciliation",
        redirect: { path: "/ops/billing", query: { tab: "reconcile" } },
        meta: {
          title: "menus.mazbri.usageReconciliation",
          isHide: true,
          roles: ["R_ADMIN", "R_FINANCE"],
        },
      }
    ],
  },
  {
    path: "/ops",
    name: "OpsMonitoring",
    component: "",
    meta: {
      title: "menus.mazbri.opsMonitoring",
      icon: "ri:bar-chart-box-line",
      order: 20,
      roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
    },
    children: [
      {
        path: "services",
        name: "OpsServices",
        component: "/mazbri/ops/services/index",
        meta: {
          title: "menus.mazbri.opsServices",
          icon: "ri:heart-pulse-line",
          keepAlive: false,
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
        },
      },
      {
        path: "services/:service",
        name: "OpsServiceDetail",
        component: "/mazbri/ops/services/detail",
        meta: {
          title: "menus.mazbri.serviceDetail",
          icon: "ri:heart-pulse-line",
          isHide: true,
          activePath: "/ops/services",
          keepAlive: false,
          roles: ["R_ADMIN"],
        },
      },
      {
        path: "traffic",
        name: "OpsTraffic",
        component: "/mazbri/ops/traffic/index",
        meta: {
          title: "menus.mazbri.opsTraffic",
          icon: "ri:dashboard-2-line",
          keepAlive: false,
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
        },
      },
      {
        path: "traffic/products",
        name: "OpsTrafficProducts",
        component: "/mazbri/ops/traffic/products",
        meta: {
          title: "menus.mazbri.productRank",
          icon: "ri:bar-chart-2-line",
          keepAlive: false,
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
        },
      },
      {
        path: "calls",
        name: "OpsCalls",
        component: "/mazbri/ops/calls/index",
        meta: {
          title: "menus.mazbri.opsCalls",
          icon: "ri:search-eye-line",
          keepAlive: false,
          roles: ["R_ADMIN", "R_FINANCE"],
        },
      },
      {
        path: "traces",
        name: "OpsTraces",
        component: "/mazbri/ops/traces/search",
        meta: {
          title: "menus.mazbri.opsTraces",
          icon: "ri:node-tree",
          keepAlive: false,
          roles: ["R_ADMIN"],
        },
      },
      {
        path: "traces/:traceId",
        name: "OpsTraceDetail",
        component: "/mazbri/ops/traces/detail",
        meta: {
          title: "menus.mazbri.opsTraceDetail",
          icon: "ri:node-tree",
          isHide: true,
          activePath: "/ops/traces",
          keepAlive: false,
          roles: ["R_ADMIN"],
        },
      },
      {
        path: "upstream",
        name: "OpsUpstream",
        component: "/mazbri/ops/upstream/index",
        meta: {
          title: "menus.mazbri.opsUpstream",
          icon: "ri:shield-check-line",
          keepAlive: false,
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
        },
      },
      {
        path: "billing",
        name: "OpsBilling",
        component: "/mazbri/ops/billing/index",
        meta: {
          title: "menus.mazbri.opsBilling",
          icon: "ri:bill-line",
          keepAlive: false,
          roles: ["R_ADMIN", "R_FINANCE"],
        },
      },
      {
        path: "flow",
        name: "OpsFlow",
        component: "/mazbri/ops/flow/index",
        meta: {
          title: "menus.mazbri.opsFlow",
          icon: "ri:speed-line",
          keepAlive: false,
          roles: ["R_ADMIN", "R_FINANCE"],
        },
      },
      {
        path: "security",
        name: "OpsSecurityEvents",
        component: "/mazbri/ops/security/index",
        meta: {
          title: "menus.mazbri.securityEvents",
          icon: "ri:shield-flash-line",
          keepAlive: false,
          roles: ["R_ADMIN", "R_FINANCE"],
        },
      },
      {
        path: "reliability",
        name: "OpsReliability",
        component: "/mazbri/ops/reliability/index",
        meta: {
          title: "menus.mazbri.opsReliability",
          icon: "ri:shield-star-line",
          keepAlive: false,
          roles: ["R_ADMIN"],
        },
      },
      {
        path: "reliability/capacity",
        name: "OpsReliabilityCapacity",
        component: "/mazbri/ops/reliability/capacity",
        meta: {
          title: "menus.mazbri.capacity",
          icon: "ri:dashboard-2-line",
          keepAlive: false,
          roles: ["R_ADMIN"],
        },
      },
      {
        path: "reliability/anomaly",
        name: "OpsReliabilityAnomaly",
        component: "/mazbri/ops/reliability/anomaly",
        meta: {
          title: "menus.mazbri.anomaly",
          icon: "ri:alarm-warning-line",
          keepAlive: false,
          roles: ["R_ADMIN", "R_FINANCE"],
        },
      },
      {
        path: "reliability/slo",
        name: "OpsReliabilitySlo",
        component: "/mazbri/ops/reliability/slo",
        meta: {
          title: "menus.mazbri.slo",
          icon: "ri:shield-check-line",
          keepAlive: false,
          roles: ["R_ADMIN"],
        },
      },
      {
        path: "infra",
        name: "OpsInfra",
        component: "/mazbri/ops/infra/index",
        meta: {
          title: "menus.mazbri.opsInfra",
          icon: "ri:server-line",
          keepAlive: false,
          roles: ["R_ADMIN"],
        },
      },
      {
        path: "alerts",
        name: "OpsAlerts",
        component: "/mazbri/ops/alerts/index",
        meta: {
          title: "menus.mazbri.opsAlerts",
          icon: "ri:alarm-warning-line",
          keepAlive: false,
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
        },
      },
      {
        path: "assets/changes",
        name: "OpsAssetsChanges",
        component: "/mazbri/ops/assets/changes",
        meta: {
          title: "menus.mazbri.assetsChanges",
          icon: "ri:git-commit-line",
          keepAlive: false,
          roles: ["R_ADMIN", "R_FINANCE"],
        },
      },
      {
        path: "tenants",
        redirect: "/operations/tenants",
        meta: {
          title: "menus.mazbri.tenantList",
          isHide: true,
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
        },
      },
      {
        path: "tenants/:code",
        redirect: (to) => `/operations/tenants/${String(to.params.code)}`,
        meta: {
          title: "menus.mazbri.tenantDetail",
          isHide: true,
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
        },
      },
      {
        path: "product-authorization",
        redirect: "/operations/product-authorization",
        meta: {
          title: "menus.mazbri.productAuthorization",
          isHide: true,
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
        },
      },
      {
        path: "products/:id",
        redirect: (to) => `/operations/products/${String(to.params.id)}`,
        meta: {
          title: "menus.mazbri.productDetail",
          isHide: true,
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
        },
      },
      {
        path: "upstream-supply",
        redirect: "/operations/upstream-supply",
        meta: {
          title: "menus.mazbri.upstreamSupply",
          isHide: true,
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
        },
      },
      {
        path: "upstream-supply/workbench",
        redirect: "/operations/upstream-supply/workbench",
        meta: {
          title: "upstreamConnector.workbenchTitle",
          isHide: true,
          roles: ["R_ADMIN", "R_FINANCE"],
        },
      },
      {
        path: "upstream-supply/suppliers/:id",
        redirect: (to) => `/operations/upstream-supply/suppliers/${String(to.params.id)}`,
        meta: {
          title: "supplier.detail.pageTitle",
          isHide: true,
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
        },
      },
      {
        path: "upstream-supply/suppliers/:id/edit",
        redirect: (to) =>
          `/operations/upstream-supply/suppliers/${String(to.params.id)}/edit`,
        meta: {
          title: "supplier.form.editTitle",
          isHide: true,
          roles: ["R_ADMIN", "R_FINANCE"],
        },
      },
      {
        path: "upstream-supply/channels/:id",
        redirect: (to) => `/operations/upstream-supply/channels/${String(to.params.id)}`,
        meta: {
          title: "channels.detail.pageTitle",
          isHide: true,
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
        },
      },
      {
        path: "upstream-supply/channels/:id/edit",
        redirect: (to) =>
          `/operations/upstream-supply/channels/${String(to.params.id)}/edit`,
        meta: {
          title: "channels.dialog.editTitle",
          isHide: true,
          roles: ["R_ADMIN", "R_FINANCE"],
        },
      },
      {
        path: "callbacks",
        redirect: "/operations/callbacks",
        meta: {
          title: "menus.mazbri.callbackList",
          isHide: true,
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
        },
      },
      {
        path: "bills",
        redirect: "/operations/bills",
        meta: {
          title: "menus.mazbri.billList",
          isHide: true,
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
        },
      },
      {
        path: "usage-reconciliation",
        redirect: { path: "/ops/billing", query: { tab: "reconcile" } },
        meta: {
          title: "menus.mazbri.usageReconciliation",
          isHide: true,
          roles: ["R_ADMIN", "R_FINANCE"],
        },
      }
    ],
  },
  
  
];
