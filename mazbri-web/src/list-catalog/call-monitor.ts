import type {
  FilterFieldDef,
  QuickFilterDef,
  ResourceFilterCatalog,
} from "@/types/list-query";
import { DATETIME_OPS, NUMBER_OPS, UUID_OPS } from "@/types/list-query";













type CallMonitorMode = "customer" | "upstream";


const FIELD = {
  timestamp: {
    field: "timestamp",
    value_type: "datetime",
    ops: DATETIME_OPS,
    in_q: false,
    label_key: "operations.monitoring.columns.responseTime",
  },
  request_id: {
    field: "request_id",
    value_type: "string",
    ops: ["eq", "contains"],
    in_q: true,
    label_key: "listFilters.fields.requestId",
  },
  trace_id: {
    field: "trace_id",
    value_type: "string",
    ops: ["eq", "contains"],
    in_q: true,
    label_key: "listFilters.fields.traceId",
  },
  tenant_code: {
    field: "tenant_code",
    value_type: "string",
    ops: ["eq", "contains"],
    in_q: true,
    label_key: "listFilters.fields.tenantCode",
  },
  tenant_id: {
    field: "tenant_id",
    value_type: "uuid",
    ops: UUID_OPS,
    in_q: true,
    label_key: "listFilters.fields.tenantId",
  },
  product_id: {
    field: "product_id",
    value_type: "string",
    ops: ["eq", "contains"],
    in_q: true,
    label_key: "listFilters.fields.productId",
  },
  supplier_code: {
    field: "supplier_code",
    value_type: "string",
    ops: ["eq", "contains"],
    in_q: true,
    label_key: "listFilters.fields.supplierCode",
  },
  channel_code: {
    field: "channel_code",
    value_type: "string",
    ops: ["eq", "contains"],
    in_q: true,
    label_key: "listFilters.fields.channelCode",
  },
  path: {
    field: "path",
    value_type: "string",
    ops: ["contains", "eq"],
    in_q: true,
    label_key: "listFilters.fields.path",
  },
  client_ip: {
    field: "client_ip",
    value_type: "string",
    ops: ["eq", "contains"],
    in_q: true,
    label_key: "listFilters.fields.clientIp",
  },
  upstream_host: {
    field: "upstream_host",
    value_type: "string",
    ops: ["eq", "contains"],
    in_q: true,
    label_key: "listFilters.fields.upstreamHost",
  },
  error_code: {
    field: "error_code",
    value_type: "string",
    ops: ["eq", "contains"],
    in_q: true,
    label_key: "listFilters.fields.errorCode",
  },
  status_code: {
    field: "status_code",
    value_type: "uint",
    ops: NUMBER_OPS,
    label_key: "listFilters.fields.statusCode",
  },
  upstream_status_code: {
    field: "upstream_status_code",
    value_type: "uint",
    ops: NUMBER_OPS,
    label_key: "listFilters.fields.upstreamStatusCode",
  },
  duration_ms: {
    field: "duration_ms",
    value_type: "uint",
    ops: ["eq", "gt", "gte", "lt", "lte", "between"],
    label_key: "listFilters.fields.durationMs",
    unit: "ms",
  },
  upstream_service_time_ms: {
    field: "upstream_service_time_ms",
    value_type: "uint",
    ops: ["eq", "gt", "gte", "lt", "lte", "between"],
    label_key: "listFilters.fields.upstreamServiceTimeMs",
    unit: "ms",
  },
} as const satisfies Record<string, FilterFieldDef>;

function withLabel(def: FilterFieldDef, label_key: string): FilterFieldDef {
  return { ...def, label_key };
}

const FULL_FIELDS: FilterFieldDef[] = Object.values(FIELD);

const STATUS_QUICK: QuickFilterDef[] = [
  {
    id: "status_gte_500",
    label_key: "listFilters.quick.statusGte500",
    filters: [{ field: "status_code", op: "gte", value: 500 }],
  },
  {
    id: "status_eq_0",
    label_key: "listFilters.quick.statusEq0",
    filters: [{ field: "status_code", op: "eq", value: 0 }],
  },
];


export const callMonitorCatalog: ResourceFilterCatalog = {
  resource: "call_monitor",
  max_filters: 16,
  fields: FULL_FIELDS,
  quick_filters: [
    {
      id: "latency_gt_100",
      label_key: "listFilters.quick.latencyGt100",
      filters: [{ field: "duration_ms", op: "gt", value: 100 }],
    },
    {
      id: "latency_gt_500",
      label_key: "listFilters.quick.latencyGt500",
      filters: [{ field: "duration_ms", op: "gt", value: 500 }],
    },
    ...STATUS_QUICK,
  ],
};





export const customerCallMonitorCatalog: ResourceFilterCatalog = {
  resource: "call_monitor_customer",
  max_filters: 16,
  fields: [
    withLabel(FIELD.request_id, "operations.monitoring.columns.serialNo"),
    withLabel(FIELD.timestamp, "operations.monitoring.columns.responseTime"),
    withLabel(FIELD.tenant_code, "operations.monitoring.columns.tenantName"),
    withLabel(FIELD.channel_code, "operations.monitoring.columns.channelName"),
    withLabel(FIELD.client_ip, "operations.monitoring.columns.requestIp"),
    withLabel(FIELD.duration_ms, "operations.monitoring.columns.customerLatency"),
    withLabel(FIELD.status_code, "operations.monitoring.columns.statusCode"),
  ],
  quick_filters: [
    {
      id: "latency_gt_100",
      label_key: "listFilters.quick.latencyGt100",
      filters: [{ field: "duration_ms", op: "gt", value: 100 }],
    },
    {
      id: "latency_gt_500",
      label_key: "listFilters.quick.latencyGt500",
      filters: [{ field: "duration_ms", op: "gt", value: 500 }],
    },
    ...STATUS_QUICK,
  ],
};





export const upstreamCallMonitorCatalog: ResourceFilterCatalog = {
  resource: "call_monitor_upstream",
  max_filters: 16,
  fields: [
    withLabel(FIELD.request_id, "operations.monitoring.columns.serialNo"),
    withLabel(FIELD.timestamp, "operations.monitoring.columns.responseTime"),
    withLabel(
      FIELD.channel_code,
      "operations.monitoring.columns.channelNameUpstream",
    ),
    withLabel(
      FIELD.upstream_service_time_ms,
      "operations.monitoring.columns.upstreamLatency",
    ),
    withLabel(FIELD.status_code, "operations.monitoring.columns.statusCode"),
  ],
  quick_filters: [
    {
      id: "upstream_latency_gt_100",
      label_key: "listFilters.quick.latencyGt100",
      filters: [{ field: "upstream_service_time_ms", op: "gt", value: 100 }],
    },
    {
      id: "upstream_latency_gt_500",
      label_key: "listFilters.quick.latencyGt500",
      filters: [{ field: "upstream_service_time_ms", op: "gt", value: 500 }],
    },
    ...STATUS_QUICK,
  ],
};

export function callMonitorCatalogFor(
  mode: CallMonitorMode,
): ResourceFilterCatalog {
  return mode === "upstream"
    ? upstreamCallMonitorCatalog
    : customerCallMonitorCatalog;
}


export const CALL_MONITOR_UNIVERSE_SIZE = 20_000;

export const CALL_MONITOR_LATEST_REFRESH_MS = 30_000;
