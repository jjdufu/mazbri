import type { AppRouteRecord } from "@/types/router";
import {
  RUNTIME_PATH,
  TRACES_PATH,
} from "@/utils/ops/opsRoutes";
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
          activePath: "/operations/tenants",
          keepAlive: false,
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
          keepAlive: false,
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
          // 财务 / 观察者可只读打开；写与发布由 UI + API 限为 R_ADMIN。
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
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
          title: "menus.mazbri.billsAndReconciliation",
          icon: "ri:file-list-3-line",
          keepAlive: false,
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
        },
      },
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
        path: "calls",
        name: "OpsCalls",
        component: "/mazbri/ops/calls/index",
        meta: {
          title: "menus.mazbri.opsCalls",
          icon: "ri:search-eye-line",
          keepAlive: false,
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
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
          activePath: TRACES_PATH,
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
        path: "runtime",
        name: "OpsRuntime",
        component: "/mazbri/ops/runtime/index",
        meta: {
          title: "menus.mazbri.opsRuntime",
          icon: "ri:pulse-line",
          keepAlive: false,
          roles: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
        },
      },
      // 旧书签临时 redirect（非业务双实现）；侧栏不可见
      {
        path: "services",
        name: "OpsServicesRedirect",
        redirect: { path: "/ops/runtime", query: { tab: "apps" } },
        meta: {
          title: "menus.mazbri.opsRuntime",
          icon: "ri:heart-pulse-line",
          isHide: true,
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
          activePath: RUNTIME_PATH,
          keepAlive: false,
          roles: ["R_ADMIN"],
        },
      },
      {
        path: "infra",
        name: "OpsInfraRedirect",
        redirect: { path: "/ops/runtime", query: { tab: "middleware" } },
        meta: {
          title: "menus.mazbri.opsRuntime",
          icon: "ri:server-line",
          isHide: true,
          keepAlive: false,
          roles: ["R_ADMIN"],
        },
      },
    ],
  },
];
