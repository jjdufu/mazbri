import type { ResourceFilterCatalog } from "@/types/list-query";
import { ENUM_OPS, NUMBER_OPS, STRING_OPS } from "@/types/list-query";

export const callbackSubscriptionsCatalog: ResourceFilterCatalog = {
  resource: "callback_subscriptions",
  max_filters: 16,
  fields: [
    {
      field: "tenant_code",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "callbacks.columns.tenantCode",
    },
    {
      field: "tenant_id",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "callbacks.columns.tenantAccount",
    },
    {
      field: "product_code",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "callbacks.columns.productCode",
    },
    {
      field: "callback_url",
      value_type: "string",
      ops: ["eq", "contains"],
      in_q: true,
      label_key: "callbacks.columns.callbackUrl",
    },
    {
      field: "is_active",
      value_type: "bool",
      ops: ["eq"],
      label_key: "common.fields.status",
    },
  ],
};

export const callbackDeliveriesCatalog: ResourceFilterCatalog = {
  resource: "callback_deliveries",
  max_filters: 16,
  fields: [
    {
      field: "event_id",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "callbacks.columns.eventId",
    },
    {
      field: "callback_url",
      value_type: "string",
      ops: ["eq", "contains"],
      in_q: true,
      label_key: "callbacks.columns.callbackUrl",
    },
    {
      field: "status",
      value_type: "enum",
      ops: ENUM_OPS,
      in_q: true,
      label_key: "common.fields.status",
      enum_values: ["pending", "success", "failed", "retrying"],
    },
    {
      field: "last_error",
      value_type: "string",
      ops: ["contains", "eq"],
      in_q: true,
      label_key: "callbacks.columns.failureReason",
    },
    {
      field: "last_response_code",
      value_type: "int",
      ops: NUMBER_OPS,
      label_key: "callbacks.columns.responseCode",
    },
    {
      field: "retry_count",
      value_type: "int",
      ops: NUMBER_OPS,
      label_key: "callbacks.columns.retryCount",
    },
  ],
};
