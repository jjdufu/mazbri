import type { ResourceFilterCatalog } from "@/types/list-query";
import {
  DATETIME_OPS,
  ENUM_OPS,
  NUMBER_OPS,
  STRING_OPS,
} from "@/types/list-query";

export const productsCatalog: ResourceFilterCatalog = {
  resource: "products",
  max_filters: 16,
  fields: [
    {
      field: "product_id",
      value_type: "string",
      ops: ["eq", "contains"],
      in_q: true,
      label_key: "product.columns.productId",
    },
    {
      field: "name",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "product.columns.name",
    },
    {
      field: "enabled",
      value_type: "bool",
      ops: ["eq"],
      label_key: "product.columns.enabled",
    },
    {
      field: "created_at",
      value_type: "datetime",
      ops: DATETIME_OPS,
      label_key: "common.fields.createdAt",
    },
  ],
};

export const suppliersCatalog: ResourceFilterCatalog = {
  resource: "suppliers",
  max_filters: 16,
  fields: [
    {
      field: "name",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "common.fields.name",
    },
    {
      field: "supplier_code",
      value_type: "string",
      ops: ["eq", "contains"],
      in_q: true,
      label_key: "listFilters.fields.supplierCode",
    },
    {
      field: "upstream_url",
      value_type: "string",
      ops: ["eq", "contains"],
      in_q: true,
      label_key: "supplier.fields.upstreamUrl",
    },
    {
      field: "status",
      value_type: "enum",
      ops: ENUM_OPS,
      label_key: "common.fields.status",
      enum_values: ["active", "disabled"],
      enum_label_keys: ["common.status.active", "common.status.disabled"],
    },
  ],
};

export const channelsCatalog: ResourceFilterCatalog = {
  resource: "channels",
  max_filters: 16,
  fields: [
    {
      field: "name",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "channels.fields.name",
    },
    {
      field: "channel_code",
      value_type: "string",
      ops: ["eq", "contains"],
      in_q: true,
      label_key: "channels.fields.channelCode",
    },
    {
      field: "supplier_code",
      value_type: "string",
      ops: ["eq", "contains"],
      in_q: true,
      label_key: "listFilters.fields.supplierCode",
    },
    {
      field: "supplier_name",
      value_type: "string",
      ops: ["eq", "contains"],
      in_q: true,
      label_key: "listFilters.fields.supplierName",
    },
    {
      field: "path_prefix",
      value_type: "string",
      ops: ["eq", "contains"],
      in_q: true,
      label_key: "channels.fields.pathPrefix",
    },
    {
      field: "status",
      value_type: "enum",
      ops: ENUM_OPS,
      label_key: "common.fields.status",
      enum_values: ["active", "disabled"],
      enum_label_keys: ["common.status.active", "common.status.disabled"],
    },
    {
      field: "qps_limit",
      value_type: "int",
      ops: NUMBER_OPS,
      label_key: "channels.fields.qpsLimit",
    },
  ],
};

export const authorizationsCatalog: ResourceFilterCatalog = {
  resource: "authorizations",
  max_filters: 16,
  fields: [
    {
      field: "tenant_code",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "authorization.list.tenantCode",
    },
    {
      field: "tenant_name",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "authorization.list.tenantName",
    },
    {
      field: "product_code",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "authorization.list.productCode",
    },
    {
      field: "product_name",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "authorization.list.productName",
    },
    {
      field: "unit_price",
      value_type: "decimal",
      ops: NUMBER_OPS,
      label_key: "authorization.list.unitPrice",
    },
    {
      field: "created_at",
      value_type: "datetime",
      ops: DATETIME_OPS,
      label_key: "authorization.list.createdAt",
    },
  ],
};

export const billsCatalog: ResourceFilterCatalog = {
  resource: "bills",
  max_filters: 16,
  fields: [
    {
      field: "id",
      value_type: "string",
      ops: ["eq", "contains"],
      in_q: true,
      label_key: "listFilters.fields.billId",
    },
    {
      field: "tenant_code",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "billing.columns.tenantCode",
    },
    {
      field: "tenant_name",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "billing.columns.tenantName",
    },
    {
      field: "bill_kind",
      value_type: "enum",
      ops: ENUM_OPS,
      in_q: true,
      label_key: "billing.columns.billKind",
      enum_values: ["customer", "upstream"],
    },
    {
      field: "status",
      value_type: "enum",
      ops: ENUM_OPS,
      in_q: true,
      label_key: "billing.columns.status",
      enum_values: [
        "draft",
        "issued",
        "paid",
        "overdue",
        "processing",
        "cancelled",
      ],
    },
    {
      field: "total",
      value_type: "decimal",
      ops: NUMBER_OPS,
      label_key: "billing.columns.total",
    },
    {
      field: "period_start",
      value_type: "datetime",
      ops: DATETIME_OPS,
      label_key: "billing.columns.periodStart",
    },
    {
      field: "period_end",
      value_type: "datetime",
      ops: DATETIME_OPS,
      label_key: "billing.columns.periodEnd",
    },
    {
      field: "created_at",
      value_type: "datetime",
      ops: DATETIME_OPS,
      label_key: "common.fields.createdAt",
    },
  ],
};

export const reconciliationCatalog: ResourceFilterCatalog = {
  resource: "usage_reconciliation",
  max_filters: 16,
  fields: [
    {
      field: "id",
      value_type: "string",
      ops: ["eq", "contains"],
      in_q: true,
      label_key: "listFilters.fields.id",
    },
    {
      field: "reconciliation_date",
      value_type: "datetime",
      ops: DATETIME_OPS,
      in_q: true,
      label_key: "billing.reconciliation.columns.date",
    },
    {
      field: "status",
      value_type: "enum",
      ops: ENUM_OPS,
      in_q: true,
      label_key: "billing.reconciliation.columns.status",
      enum_values: ["ok", "flagged", "error", "acknowledged", "resolved"],
    },
    {
      field: "scope_type",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "listFilters.fields.scope",
    },
    {
      field: "scope_id",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "listFilters.fields.scope",
    },
    {
      field: "billable_call_count",
      value_type: "int",
      ops: NUMBER_OPS,
      label_key: "billing.reconciliation.columns.oltp",
    },
    {
      field: "gateway_call_count",
      value_type: "int",
      ops: NUMBER_OPS,
      label_key: "billing.reconciliation.columns.gateway",
    },
    {
      field: "upstream_preagg_call_count",
      value_type: "int",
      ops: NUMBER_OPS,
      label_key: "billing.reconciliation.columns.olap",
    },
    {
      field: "discrepancy_ratio",
      value_type: "decimal",
      ops: NUMBER_OPS,
      label_key: "billing.reconciliation.columns.ratio",
    },
    {
      field: "created_at",
      value_type: "datetime",
      ops: DATETIME_OPS,
      label_key: "common.fields.createdAt",
    },
  ],
};

export const usersCatalog: ResourceFilterCatalog = {
  resource: "admins",
  max_filters: 16,
  fields: [
    {
      field: "username",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "userManagement.list.columns.username",
    },
    {
      field: "display_name",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "userManagement.list.columns.displayName",
    },
    {
      field: "email",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "userManagement.list.columns.email",
    },
    {
      field: "role",
      value_type: "enum",
      ops: ENUM_OPS,
      label_key: "userManagement.list.columns.role",
      enum_values: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
    },
    {
      field: "status",
      value_type: "enum",
      ops: ENUM_OPS,
      label_key: "userManagement.list.columns.status",
      enum_values: ["active", "inactive"],
      enum_label_keys: ["common.status.active", "common.status.disabled"],
    },
    {
      field: "mfa_enabled",
      value_type: "bool",
      ops: ["eq"],
      label_key: "userManagement.list.columns.mfa",
    },
    {
      field: "last_login_at",
      value_type: "datetime",
      ops: DATETIME_OPS,
      label_key: "userManagement.list.columns.lastLogin",
    },
    {
      field: "created_at",
      value_type: "datetime",
      ops: DATETIME_OPS,
      label_key: "userManagement.list.columns.createdAt",
    },
  ],
};

export const routesCatalog: ResourceFilterCatalog = {
  resource: "routes",
  max_filters: 16,
  fields: [
    {
      field: "tenant_id",
      value_type: "uuid",
      ops: ["eq", "in"],
      in_q: true,
      label_key: "routes.columns.tenant",
    },
    {
      field: "product_id",
      value_type: "uuid",
      ops: ["eq", "in"],
      in_q: true,
      label_key: "routes.columns.product",
    },
    {
      field: "tenant_code",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "routes.columns.tenant",
    },
    {
      field: "tenant_name",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "routes.columns.tenant",
    },
    {
      field: "product_code",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "routes.columns.product",
    },
    {
      field: "product_name",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "routes.columns.product",
    },
    {
      field: "status",
      value_type: "enum",
      ops: ENUM_OPS,
      label_key: "routes.columns.status",
      enum_values: ["active", "inactive", "suspended"],
    },
    {
      field: "priority",
      value_type: "int",
      ops: NUMBER_OPS,
      label_key: "routes.columns.priority",
    },
    {
      field: "created_at",
      value_type: "datetime",
      ops: DATETIME_OPS,
      label_key: "common.fields.createdAt",
    },
  ],
};

export const configCenterCatalog: ResourceFilterCatalog = {
  resource: "config_center",
  max_filters: 16,
  fields: [
    {
      field: "service_name",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "configCenter.columns.serviceName",
    },
    {
      field: "updated_by",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "configCenter.columns.updatedBy",
    },
    {
      field: "updated_at",
      value_type: "datetime",
      ops: DATETIME_OPS,
      label_key: "configCenter.columns.updatedAt",
    },
  ],
};

export const auditLogsCatalog: ResourceFilterCatalog = {
  resource: "audit_logs",
  max_filters: 16,
  fields: [
    {
      field: "actor_name",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "auditLogs.columns.actorName",
    },
    {
      field: "action",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "auditLogs.columns.action",
    },
    {
      field: "resource_type",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "auditLogs.columns.resourceType",
    },
    {
      field: "status",
      value_type: "enum",
      ops: ENUM_OPS,
      label_key: "auditLogs.columns.status",
      enum_values: ["success", "failure", "denied", "error"],
    },
    {
      field: "failure_reason",
      value_type: "string",
      ops: ["eq", "contains"],
      in_q: true,
      label_key: "auditLogs.columns.failureReason",
    },
    {
      field: "request_id",
      value_type: "string",
      ops: ["eq"],
      in_q: true,
      label_key: "auditLogs.columns.requestId",
    },
    {
      field: "ip_address",
      value_type: "string",
      ops: ["eq", "contains"],
      in_q: true,
      label_key: "auditLogs.columns.ipAddress",
    },
    {
      field: "timestamp",
      value_type: "datetime",
      ops: DATETIME_OPS,
      label_key: "auditLogs.columns.timestamp",
    },
  ],
};
