import type { ResourceFilterCatalog } from "@/types/list-query";
import { DATETIME_OPS, ENUM_OPS, STRING_OPS } from "@/types/list-query";


export const tenantsCatalog: ResourceFilterCatalog = {
  resource: "tenants",
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
      field: "tenant_id",
      value_type: "string",
      ops: ["eq", "contains"],
      in_q: true,
      label_key: "tenant.fields.accountId",
    },
    {
      field: "tenant_code",
      value_type: "string",
      ops: ["eq", "contains"],
      in_q: true,
      label_key: "tenant.fields.code",
    },
    {
      field: "contact_email",
      value_type: "string",
      ops: ["eq", "contains"],
      in_q: true,
      label_key: "tenant.fields.contactEmail",
    },
    {
      field: "contact_person",
      value_type: "string",
      ops: ["eq", "contains"],
      in_q: true,
      label_key: "tenant.fields.contactPerson",
    },
    {
      field: "contact_phone",
      value_type: "string",
      ops: ["eq", "contains"],
      in_q: true,
      label_key: "tenant.fields.contactPhone",
    },
    {
      field: "status",
      value_type: "enum",
      ops: ENUM_OPS,
      label_key: "common.fields.status",
      enum_values: ["active", "suspended", "pending"],
      enum_label_keys: [
        "common.status.active",
        "common.status.suspended",
        "common.status.pending",
      ],
    },
    {
      field: "created_at",
      value_type: "datetime",
      ops: DATETIME_OPS,
      label_key: "common.fields.createdAt",
    },
  ],
};
