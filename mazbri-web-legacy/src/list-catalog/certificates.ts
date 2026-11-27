import type { ResourceFilterCatalog } from "@/types/list-query";
import { DATETIME_OPS, ENUM_OPS, STRING_OPS } from "@/types/list-query";


export const certificatesCatalog: ResourceFilterCatalog = {
  resource: "certificates",
  max_filters: 16,
  fields: [
    {
      field: "subject",
      value_type: "string",
      ops: STRING_OPS,
      in_q: true,
      label_key: "certificate.list.columns.subject",
    },
    {
      field: "fingerprint",
      value_type: "string",
      ops: ["eq", "contains"],
      in_q: true,
      label_key: "listFilters.fields.fingerprint",
    },
    {
      field: "dns_names",
      value_type: "string",
      ops: ["contains", "eq"],
      in_q: true,
      label_key: "certificate.list.columns.dnsNames",
    },
    {
      field: "expires_at",
      value_type: "datetime",
      ops: DATETIME_OPS,
      label_key: "certificate.list.columns.expiresAt",
    },
    {
      field: "status",
      value_type: "enum",
      ops: ENUM_OPS,
      label_key: "common.fields.status",
      enum_values: ["valid", "expiring_soon", "expired", "revoked"],
      enum_label_keys: [
        "common.statusLabels.valid",
        "common.statusLabels.expiringSoon",
        "common.statusLabels.expired",
        "common.statusLabels.revoked",
      ],
    },
  ],
};
