<template>
  <div ref="tablePageRef" class="call-monitor-page" style="display: flex; flex-direction: column; flex: 1; min-height: 0;">
    <ResourceTableShell
      v-model:columns="columns"
      :title="title"
      :loading="loading"
      :data="tableData"
      :pagination="artPagination"
      :show-total="false"
      :empty-text="emptyDescription"
      :fit="true"
      table-layout="fixed"
      @refresh="onManualRefresh"
      @selection-change="handleSelectionChange"
      @pagination:size-change="handlePageSizeChange"
      @pagination:current-change="handleCurrentPageChange"
    >

      <template #serial_no="{ row }">
        <span class="serial-no-cell">{{ row.serial_no || "-" }}</span>
      </template>
      <template #actions>
        <ListQueryBar
          v-model:q="q"
          v-model:filters="filters"
          :catalog="listCatalog"
          @search="onQueryBarSearch"
        />
        <TableExportDropdown
          :loading="exporting"
          :items="exportItems"
          :label="$t('operations.monitoring.actions.export')"
          @command="
            (command) =>
              handleExportCommand(command as 'page' | 'selected' | 'filtered')
          "
        />
        <ElButton @click="resetSearch">{{ $t("common.reset") }}</ElButton>
      </template>

      <template #tenant_name="{ row }">
        {{ row.tenant_name || row.tenant_id || "-" }}
      </template>
      <template #channel_name="{ row }">
        <ElTooltip
          :content="channelNameHoverContent(row)"
          placement="top"
          :disabled="!channelNameHoverContent(row)"
          :show-after="0"
          :hide-after="0"
          :enterable="false"
          transition="call-monitor-tooltip-instant"
          popper-class="call-monitor-channel-tooltip"
        >
          <span class="channel-name-cell">{{ formatChannelName(row) }}</span>
        </ElTooltip>
      </template>
      <template #response_time="{ row }">
        {{ formatTimestampWithMillis(row.response_time) }}
      </template>
      <template #request_ip="{ row }">
        {{ row.request_ip || row.client_ip || "-" }}
      </template>
      <template #duration_ms="{ row }"> {{ row.duration_ms }} ms </template>
      <template #upstream_service_time_ms="{ row }">
        {{ row.upstream_service_time_ms }} ms
      </template>
      <template #status_code="{ row }">
        <ElTag :type="statusTagType(row.status_code)">
          {{ row.status_code || "-" }}
        </ElTag>
      </template>
      <template #response_result="{ row }">
        {{ getCallMonitorResponseResult(row) }}
      </template>
      <template #operations="{ row }">
        <TableActionRow>
          <ElButton link type="primary" @click="showDetail(row)">{{
            $t("common.actions.details")
          }}</ElButton>
        </TableActionRow>
      </template>
    </ResourceTableShell>

    <AdaptiveDrawer
      v-model="detailVisible"
      :title="$t('operations.monitoring.detail.title')"
      destroy-on-close
    >
      <template #header>
        <CommonDrawerHeader
          :title="$t('operations.monitoring.detail.title')"
          :subtitle="currentDetail?.serial_no || '-'"
        />
      </template>

      <div v-if="currentDetail" class="detail-drawer-body">
        <div class="detail-toolbar">
          <ElButton
            v-if="currentDetail"
            type="primary"
            plain
            @click="downloadDetailJson"
          >
            <template #icon><ArtSvgIcon icon="ri:download-line" /></template>
            {{ $t("operations.monitoring.actions.downloadJson") }}
          </ElButton>
        </div>

        <ElRow :gutter="16">
          <ElCol :xs="24" :lg="24">
            <ElCard shadow="never" class="detail-card">
              <template #header>
                <SurfaceCardHeader
                  :title="$t('operations.monitoring.detail.summaryTitle')"
                />
              </template>

              <ElDescriptions :column="1" border size="small">
                <ElDescriptionsItem
                  :label="$t('operations.monitoring.detail.labels.serialNo')"
                >
                  {{ currentDetail.serial_no }}
                </ElDescriptionsItem>
                <ElDescriptionsItem
                  :label="$t('operations.monitoring.detail.labels.party')"
                >
                  {{
                    props.mode === "upstream"
                      ? currentDetail.supplier ||
                        currentDetail.supplier_name ||
                        "-"
                      : currentDetail.tenant_name ||
                        currentDetail.tenant_id ||
                        "-"
                  }}
                </ElDescriptionsItem>
                <ElDescriptionsItem
                  :label="$t('operations.monitoring.detail.labels.channelName')"
                >
                  {{ formatChannelName(currentDetail) }}
                </ElDescriptionsItem>
                <ElDescriptionsItem
                  :label="
                    $t('operations.monitoring.detail.labels.responseTime')
                  "
                >
                  {{ formatTimestampWithMillis(currentDetail.response_time) }}
                </ElDescriptionsItem>
                <ElDescriptionsItem
                  v-if="props.mode === 'customer'"
                  :label="$t('operations.monitoring.columns.customerLatency')"
                >
                  {{ currentDetail.duration_ms }} ms
                </ElDescriptionsItem>
                <ElDescriptionsItem
                  v-else
                  :label="$t('operations.monitoring.columns.upstreamLatency')"
                >
                  {{ currentDetail.upstream_service_time_ms }} ms
                </ElDescriptionsItem>
                <ElDescriptionsItem label="内部处理耗时">
                  {{ currentDetail.internal_ms }} ms
                </ElDescriptionsItem>
                <ElDescriptionsItem label="后置耗时">
                  {{ currentDetail.post_response_ms }} ms
                </ElDescriptionsItem>
                <ElDescriptionsItem
                  :label="$t('operations.monitoring.detail.labels.statusCode')"
                >
                  <ElTag :type="statusTagType(currentDetail.status_code)">
                    {{ currentDetail.status_code }}
                  </ElTag>
                </ElDescriptionsItem>
                <ElDescriptionsItem
                  v-if="props.mode === 'customer'"
                  :label="$t('operations.monitoring.detail.labels.requestIp')"
                >
                  {{
                    currentDetail.request_ip || currentDetail.client_ip || "-"
                  }}
                </ElDescriptionsItem>
                <ElDescriptionsItem
                  v-if="props.mode === 'customer'"
                  :label="
                    $t('operations.monitoring.detail.labels.responseResult')
                  "
                >
                  {{ getCallMonitorResponseResult(currentDetail) }}
                </ElDescriptionsItem>
              </ElDescriptions>
            </ElCard>

            <ElCard shadow="never" class="detail-card detail-card--compact">
              <template #header>
                <SurfaceCardHeader
                  :title="$t('operations.monitoring.detail.rawTitle')"
                />
              </template>

              <ElDescriptions :column="1" border size="small">
                <ElDescriptionsItem
                  v-for="item in detailEntries"
                  :key="item.key"
                  :label="item.key"
                >
                  <ElTooltip
                    :disabled="!item.truncated"
                    :content="item.full"
                    placement="top-start"
                    :show-after="300"
                  >
                    <span class="detail-value">{{ item.display }}</span>
                  </ElTooltip>
                </ElDescriptionsItem>
                <ElDescriptionsItem
                  v-if="detailEntries.length === 0"
                  label="detail"
                >
                  {{ $t("operations.monitoring.detail.empty") }}
                </ElDescriptionsItem>
              </ElDescriptions>
            </ElCard>
          </ElCol>
        </ElRow>
      </div>
    </AdaptiveDrawer>
  </div>
</template>

<script setup lang="ts">
import ResourceTableShell from "@/components/common/resource-table-shell/index.vue";
import ListQueryBar from "@/components/common/list-query-bar/index.vue";
import CommonDrawerHeader from "@/components/common/drawer-header/index.vue";
import SurfaceCardHeader from "@/components/common/surface-card-header/index.vue";
import TableExportDropdown from "@/components/table/TableExportDropdown.vue";
import { downloadBinary, createTarArchive, gzipBytes } from "@/utils/archive";
import AdaptiveDrawer from "@/components/common/AdaptiveDrawer.vue";
import { useAdaptiveTableColumns } from "@/composables/useAdaptiveTableColumns";
import { useListQuery } from "@/composables/useListQuery";
import {
  CALL_MONITOR_LATEST_REFRESH_MS,
  CALL_MONITOR_UNIVERSE_SIZE,
  callMonitorCatalogFor,
} from "@/list-catalog";
import {
  fetchCustomerCallLogs,
  fetchUpstreamCallLogs,
} from "@/api/call-monitor";
import { runWithLoading } from "@/composables/useAsyncState";
import { useTableSelection } from "@/composables/useTableSelection";
import { logger } from "@/utils/sys";
import { formatUrlDisplay, sanitizeSensitiveValue } from "@/utils/sensitive";
import type {
  CallMonitorItem,
  CallMonitorQuery,
  CallMonitorQueryMode,
  CallMonitorResponse,
} from "@/types/api/call-monitor";
import type { ListFilter } from "@/types/list-query";
import type { ColumnOption, PaginationConfig } from "@/types/component";
import type { CallMonitorModeHintPayload } from "./CallMonitorModeHint.vue";
import {
  clampPageToSafeRange,
  MAX_TABLE_BROWSE_TOTAL,
  resolveSafePageCount,
} from "@/utils/table/paginationLimits";
import { $t } from "@/locales";
import {
  parseRequestIdLookup,
  requestIdFromFilters,
} from "@/utils/list-query/request-id";

const TIME_ZONE = "Asia/Shanghai";


const timestampFiltersFromRouteQuery = (
  query: Record<string, unknown>,
): ListFilter[] => {
  const parseTs = (raw: unknown): number | null => {
    if (raw === null || raw === undefined || raw === "") return null;
    const n = Number(Array.isArray(raw) ? raw[0] : raw);
    if (!Number.isFinite(n) || n <= 0) return null;
    return n < 1_000_000_000_000 ? Math.floor(n * 1000) : Math.floor(n);
  };
  const start = parseTs(query.start_time);
  const end = parseTs(query.end_time);
  if (start === null && end === null) return [];
  const out: ListFilter[] = [];
  if (start !== null) {
    out.push({ field: "timestamp", op: "gte", value: formatWallClock(start) });
  }
  if (end !== null) {
    out.push({ field: "timestamp", op: "lte", value: formatWallClock(end) });
  }
  return out;
};

const formatWallClock = (ms: number): string => {
  const parts = new Intl.DateTimeFormat("sv-SE", {
    timeZone: TIME_ZONE,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  }).formatToParts(new Date(ms));
  const g = Object.fromEntries(parts.map((p) => [p.type, p.value]));
  return `${g.year}-${g.month}-${g.day} ${g.hour}:${g.minute}:${g.second}`;
};

const hasTimestampFilter = (list: ListFilter[]): boolean =>
  list.some((f) => f.field === "timestamp");

const props = defineProps<{
  mode: "customer" | "upstream";
  title: string;
}>();
const route = useRoute();
const tablePageRef = ref<HTMLElement | null>(null);
const loading = ref(false);
const tableData = ref<CallMonitorItem[]>([]);
const detailVisible = ref(false);
const currentDetail = ref<CallMonitorItem | null>(null);
const exporting = ref(false);
const pagination = reactive({
  page: 1,
  page_size: 20,
  total: 0,
  total_pages: 0,
});
const lastQueryMode = ref<CallMonitorQueryMode | string>("latest");
const lastUniverseSize = ref(CALL_MONITOR_UNIVERSE_SIZE);
const dataAsOf = ref<number | null>(null);
const browseCapped = ref(false);

const listCatalog = computed(() => callMonitorCatalogFor(props.mode));
const { q, filters, toRequestParams } = useListQuery({
  catalog: listCatalog,
  searchOnQChange: false,
  onChange: () => {},
  encodeMode: "both",
});


{
  const seeded = timestampFiltersFromRouteQuery(
    route.query as Record<string, unknown>,
  );
  if (seeded.length) {
    filters.value = [...filters.value, ...seeded];
  }
}

const DETAIL_EXPORT_EXCLUDED_KEYS = new Set([
  "service",
  "instance_id",
  "upstream_host",
  "upstream_status_code",
  "level",
  "cache_hit",
]);

const onQueryBarSearch = () => {
  pagination.page = 1;
  loadData();
};

const onManualRefresh = () => {
  loadData();
};

const refreshInSec = ref(0);

const nextRefreshAtMs = ref<number | null>(null);


const modeHint = computed((): CallMonitorModeHintPayload => {
  const mode = lastQueryMode.value || "latest";
  if (mode === "selective") {
    return {
      kind: "text",
      text: $t("operations.monitoring.mode.selective"),
    };
  }
  if (mode === "ranged") {
    const base = $t("operations.monitoring.mode.ranged");
    const text = browseCapped.value
      ? `${base} · ${$t("operations.monitoring.browse.cappedTitle")}`
      : base;
    return { kind: "text", text };
  }
  const asOf =
    dataAsOf.value != null
      ? formatWallClock(dataAsOf.value)
      : $t("operations.monitoring.mode.unknownTime");
  return {
    kind: "latest",
    asOf,
    size: lastUniverseSize.value || CALL_MONITOR_UNIVERSE_SIZE,
    refreshIn: refreshInSec.value,
  };
});

const emit = defineEmits<{

  modeHintChange: [hint: CallMonitorModeHintPayload];
}>();
watch(
  modeHint,
  (hint) => {
    emit("modeHintChange", hint);
  },
  { immediate: true, deep: true },
);

const tickRefreshCountdown = () => {
  const deadline = nextRefreshAtMs.value;
  if (deadline == null) {
    refreshInSec.value = 0;
    return;
  }
  const leftMs = deadline - Date.now();
  refreshInSec.value = Math.max(0, Math.ceil(leftMs / 1000));
};


const canScheduleAutoRefresh = () => {

  if (hasTimestampFilter(filters.value)) {
    return false;
  }
  if (parseRequestIdLookup(q.value) || requestIdFromFilters(filters.value)) {
    return false;
  }
  if (lastQueryMode.value && lastQueryMode.value !== "latest") {
    return false;
  }
  return true;
};

const armLatestRefreshCountdown = () => {
  if (!canScheduleAutoRefresh()) {
    nextRefreshAtMs.value = null;
    refreshInSec.value = 0;
    return;
  }
  nextRefreshAtMs.value = Date.now() + CALL_MONITOR_LATEST_REFRESH_MS;
  tickRefreshCountdown();
};

const TIMESTAMP_PARTS_FORMATTER = new Intl.DateTimeFormat(undefined, {
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
  hour12: false,
  timeZone: TIME_ZONE,
});






const formatTimestampWithMillis = (value: string | number | null) => {
  if (!value) return "-";
  const raw = String(value);
  const date =
    typeof value === "number"
      ? new Date(value > 1_000_000_000_000_000 ? value / 1_000_000 : value)
      : new Date(value);
  if (Number.isNaN(date.getTime())) return raw;
  const parts = TIMESTAMP_PARTS_FORMATTER.formatToParts(date);
  const lookup = Object.fromEntries(
    parts.map((part) => [part.type, part.value]),
  );
  const milliDigits = String(date.getMilliseconds()).padStart(3, "0");
  return `${lookup.year}-${lookup.month}-${lookup.day} ${lookup.hour || "00"}:${lookup.minute || "00"}:${lookup.second || "00"}.${milliDigits}`;
};

const getCallMonitorResponseResult = (
  row: CallMonitorItem | null | undefined,
) => {
  if (!row) return "-";
  const detail = (row.detail || {}) as Record<string, unknown>;
  const fromDetail = (key: string) =>
    typeof detail[key] === "string" ? (detail[key] as string) : "";
  const value =
    row.response_result ||
    row.response_flags ||
    fromDetail("response_result") ||
    fromDetail("response_flags") ||
    "";
  return value.trim() || "-";
};


const formatChannelName = (row: CallMonitorItem | null | undefined) => {
  if (!row) return "-";
  const detail = (row.detail || {}) as Record<string, unknown>;
  const cacheHit = detail.cache_hit === true;
  const fallbackUsed = detail.fallback_used === true;
  const raw = String(row.channel_name || row.channel_code || "").trim();
  if (!raw) {
    return cacheHit || fallbackUsed ? "CB" : "-";
  }
  if (raw.startsWith("CB:") || raw.startsWith("CB/") || raw === "CB") {
    return raw;
  }
  if (cacheHit || fallbackUsed) {
    return `CB:${raw}`;
  }
  return raw;
};


const channelNameHoverContent = (row: CallMonitorItem | null | undefined) => {
  if (!row) return "";
  const channel = formatChannelName(row);
  if (channel === "-") return "";

  if (props.mode !== "upstream") return channel;

  const channelLine = `${$t("operations.monitoring.columns.channelName")}: ${channel}`;
  const supplier = String(row.supplier || row.supplier_name || "").trim();
  if (!supplier) return channelLine;
  return `${channelLine}\n${$t("operations.monitoring.columns.supplier")}: ${supplier}`;
};

const getCallMonitorCellText = (
  columnKey: string,
  row: CallMonitorItem,
  currentMode: "customer" | "upstream",
) => {
  switch (columnKey) {
    case "serial_no":
      return row.serial_no || "-";
    case "supplier":
      return row.supplier || row.supplier_name || "-";
    case "tenant_name":
      return row.tenant_name || row.tenant_id || "-";
    case "channel_name":
      return formatChannelName(row);
    case "response_time":
      return formatTimestampWithMillis(row.response_time);
    case "request_ip":
      return row.request_ip || row.client_ip || "-";
    case "duration_ms":
      return `${row.duration_ms} ms`;
    case "upstream_service_time_ms":
      return `${row.upstream_service_time_ms} ms`;
    case "status_code":
      return String(row.status_code ?? "-");
    case "response_result":
      return getCallMonitorResponseResult(row);
    case "operations":
      return $t("common.actions.details");
    default:
      return currentMode === "customer"
        ? row.tenant_name || row.tenant_id || "-"
        : row.supplier || row.supplier_name || "-";
  }
};




const getCallMonitorColumnWeight = (
  columnKey: string,
  mode: "grow" | "shrink" = "grow",
) => {
  if (mode === "shrink") {
    switch (columnKey) {
      case "serial_no":
        return 0.08;
      case "channel_name":
      case "tenant_name":
      case "response_result":
        return 2.4;
      case "response_time":
        return 1.1;
      case "request_ip":
        return 0.6;
      case "duration_ms":
      case "upstream_service_time_ms":
      case "status_code":
        return 0.5;
      default:
        return 1;
    }
  }

  switch (columnKey) {
    case "serial_no":
      return 0.4;
    case "channel_name":
    case "tenant_name":
      return 0.35;
    case "response_time":
      return 1.6;
    case "request_ip":
      return 1.2;
    case "duration_ms":
    case "upstream_service_time_ms":
      return 0.8;
    case "status_code":
      return 0.6;
    case "response_result":
      return 1.5;
    default:
      return 1;
  }
};
const extractRawFields = (
  detail: Record<string, unknown> | null | undefined,
) => {
  if (!detail || typeof detail !== "object" || Array.isArray(detail)) {
    return {};
  }
  const raw = detail.raw;
  if (typeof raw === "string" && raw.trim()) {
    try {
      const parsed = JSON.parse(raw) as Record<string, unknown>;
      const fields = parsed.fields;
      if (fields && typeof fields === "object" && !Array.isArray(fields)) {
        return fields as Record<string, unknown>;
      }
    } catch {
      return {};
    }
  }
  return {};
};
const buildDetailPayload = (
  detail: Record<string, unknown> | null | undefined,
) => {
  const rawFields = extractRawFields(detail);
  const filtered = Object.fromEntries(
    Object.entries(rawFields).filter(
      ([key]) => !DETAIL_EXPORT_EXCLUDED_KEYS.has(key),
    ),
  );
  return sanitizeSensitiveValue(filtered);
};
const detailEntries = computed(() => {
  const payload = buildDetailPayload(currentDetail.value?.detail);
  return Object.entries(payload).map(([key, value]) => ({
    key,
    ...formatDetailEntryValue(value),
  }));
});
const hasActiveFilter = computed(
  () => Boolean(q.value.trim()) || filters.value.length > 0,
);
const emptyDescription = computed(() => {
  if (!hasActiveFilter.value) {
    return $t("operations.monitoring.empty.noData");
  }
  if (
    (lastQueryMode.value === "latest" || !lastQueryMode.value) &&
    !hasTimestampFilter(filters.value)
  ) {
    return $t("operations.monitoring.empty.noMatchLatest", {
      size: lastUniverseSize.value || CALL_MONITOR_UNIVERSE_SIZE,
    });
  }
  return $t("operations.monitoring.empty.noMatch");
});
const artPagination = computed<PaginationConfig>(() => ({
  current: pagination.page,
  size: pagination.page_size,
  total: pagination.total,
  pageCount: pagination.total_pages || undefined,
}));
const exportItems = computed(() => [
  {
    command: "page",
    label: $t("operations.monitoring.actions.exportPage"),
    disabled: tableData.value.length === 0,
  },
  {
    command: "selected",
    label: $t("operations.monitoring.actions.exportSelected", {
      count: selectedRows.value.length,
    }),
    disabled: selectedRows.value.length === 0,
  },
  {
    command: "filtered",
    label: $t("operations.monitoring.actions.exportFiltered"),
  },
]);
const { selectedRows, handleSelectionChange } =
  useTableSelection<CallMonitorItem>({
    deleteApi: async () => {},
    getItemName: (row) => row.serial_no,
    getItemId: (row) => row.serial_no,
    actionLabel: $t("operations.monitoring.actions.export"),
  });
const buildColumns = (
  currentMode: "customer" | "upstream",
): ColumnOption<CallMonitorItem>[] => {
  const columns: ColumnOption<CallMonitorItem>[] = [
    { type: "selection", width: 48 },
    {
      prop: "serial_no",
      label: $t("operations.monitoring.columns.serialNo"),

      minWidth: 310,
      maxWidth: 340,
      useSlot: true,
      showOverflowTooltip: false,
    },
  ];

  if (currentMode === "customer") {
    columns.push({
      prop: "tenant_name",
      label: $t("operations.monitoring.columns.tenantName"),
      minWidth: 72,
      maxWidth: 120,
      useSlot: true,
      showOverflowTooltip: true,
    });
  }
  columns.push(
    {
      prop: "channel_name",
      label:
        currentMode === "upstream"
          ? $t("operations.monitoring.columns.channelNameUpstream")
          : $t("operations.monitoring.columns.channelName"),

      minWidth: 100,
      maxWidth: 180,
      useSlot: true,
      showOverflowTooltip: false,
    },
    {
      prop: "response_time",
      label: $t("operations.monitoring.columns.responseTime"),
      minWidth: 180,
      maxWidth: 220,
      useSlot: true,
    },
  );
  if (currentMode === "customer") {
    columns.push({
      prop: "request_ip",
      label: $t("operations.monitoring.columns.requestIp"),

      minWidth: 120,
      maxWidth: 148,
      useSlot: true,
      showOverflowTooltip: true,
    });
  }
  if (currentMode === "customer") {
    columns.push({
      prop: "duration_ms",
      label: $t("operations.monitoring.columns.customerLatency"),
      minWidth: 80,
      maxWidth: 100,
      useSlot: true,
    });
  } else {
    columns.push({
      prop: "upstream_service_time_ms",
      label: $t("operations.monitoring.columns.upstreamLatency"),
      minWidth: 80,
      maxWidth: 100,
      useSlot: true,
    });
  }
  columns.push({
    prop: "status_code",
    label: $t("operations.monitoring.columns.statusCode"),
    minWidth: 72,
    maxWidth: 96,
    useSlot: true,
  });
  if (currentMode === "customer") {
    columns.push({
      prop: "response_result",
      label: $t("operations.monitoring.columns.responseResult"),
      minWidth: 100,
      maxWidth: 200,
      useSlot: true,
      showOverflowTooltip: true,
    });
  }
  columns.push({
    prop: "operations",
    label: $t("common.fields.operations"),
    minWidth: 80,
    width: 80,
    fixed: "right",
    useSlot: true,
  });
  return columns;
};
const columns = ref<ColumnOption<CallMonitorItem>[]>([]);
useAdaptiveTableColumns({
  columns,
  containerRef: tablePageRef,
  rows: tableData,
  baseColumns: () => buildColumns(props.mode),
  selectionWidth: 48,
  operationsWidth: 80,
  softMinCap: 168,
  fitContainer: true,
  getCellText: ({ columnKey, row }) =>
    getCallMonitorCellText(columnKey, row, props.mode),
  getColumnWeight: ({ columnKey, mode }) =>
    getCallMonitorColumnWeight(columnKey, mode),
});
const applyResponseMeta = (response: CallMonitorResponse) => {
  const total = response.total || 0;
  pagination.total = total;
  pagination.total_pages =
    response.total_pages ||
    resolveSafePageCount(total, pagination.page_size);
  lastQueryMode.value = response.query_mode || "latest";
  lastUniverseSize.value =
    response.universe_size || CALL_MONITOR_UNIVERSE_SIZE;
  browseCapped.value = Boolean(response.browse_capped);
  dataAsOf.value = Date.now();

  if ((response.query_mode || "latest") === "latest") {
    armLatestRefreshCountdown();
  } else {
    nextRefreshAtMs.value = null;
    refreshInSec.value = 0;
  }
};

const loadData = async (opts?: { silent?: boolean }) => {
  const requestedPage = pagination.page;
  const run = async () => {
    const response = await fetchLogs(buildQuery());
    const rows = response.data || [];
    const total = response.total || 0;
    const totalPages =
      response.total_pages ||
      resolveSafePageCount(total, pagination.page_size);
    const safePage = clampPageToSafeRange(
      requestedPage,
      total,
      pagination.page_size,
      totalPages,
    );

    if (rows.length === 0 && total > 0 && safePage !== requestedPage) {
      pagination.page = safePage;
      applyResponseMeta(response);
      const retry = await fetchLogs(buildQuery(safePage));
      tableData.value = retry.data || [];
      applyResponseMeta(retry);
      return;
    }

    tableData.value = rows;
    applyResponseMeta(response);
    if (safePage !== requestedPage) {
      pagination.page = safePage;
    }
  };

  try {
    if (opts?.silent) {
      await run();
    } else {
      await runWithLoading(loading, run);
    }
  } catch (error) {
    logger.error("Failed to load call monitor logs:", error);
    if (!opts?.silent) {
      ElMessage.error($t("common.table.fetchFailed"));
    }
  }
};
const buildQuery = (
  page = pagination.page,
  pageSize = pagination.page_size,
): CallMonitorQuery => {


  const selectiveSerial =
    parseRequestIdLookup(q.value) ?? requestIdFromFilters(filters.value);

  if (selectiveSerial) {
    const params = toRequestParams({
      page,
      page_size: pageSize,
      serial_no: selectiveSerial,
    }) as CallMonitorQuery;


    delete params.q;

    delete params.start_time;
    delete params.end_time;
    return params;
  }



  return toRequestParams({
    page,
    page_size: pageSize,
  }) as CallMonitorQuery;
};
const fetchLogs = (query: CallMonitorQuery): Promise<CallMonitorResponse> =>
  props.mode === "upstream"
    ? fetchUpstreamCallLogs(query)
    : fetchCustomerCallLogs(query);
const resetSearch = () => {
  q.value = "";
  filters.value = [];
  pagination.page = 1;
  lastQueryMode.value = "latest";
  loadData();
};
const handlePageSizeChange = (size: number) => {
  pagination.page_size = size;
  pagination.page = 1;

  pagination.total_pages = 0;
  loadData();
};
const handleCurrentPageChange = (page: number) => {
  pagination.page = clampPageToSafeRange(
    page,
    pagination.total,
    pagination.page_size,
    pagination.total_pages || undefined,
  );
  loadData();
};
const showDetail = (row: CallMonitorItem) => {
  currentDetail.value = row;
  detailVisible.value = true;
};
const downloadDetailJson = () => {
  if (!currentDetail.value) return;
  const json = JSON.stringify(
    buildDetailPayload(currentDetail.value.detail),
    null,
    2,
  );
  downloadBinary(
    `${props.mode}-call-detail-${buildSafeFileName(currentDetail.value.serial_no || "record")}.json`,
    `\uFEFF${json}`,
    "application/json;charset=utf-8;",
  );
};
const handleExportCommand = async (
  command: "page" | "selected" | "filtered",
) => {
  if (exporting.value) return;
  if (command === "selected") {
    await exportLogsArchive(selectedRows.value, "selected");
    return;
  }
  if (command === "page") {
    await exportLogsArchive(tableData.value, "page");
    return;
  }
  await exportFilteredLogsArchive();
};
const exportFilteredLogsArchive = async () => {
  await runWithLoading(exporting, async () => {
    const response = await fetchLogs(
      buildQuery(1, Math.min(MAX_TABLE_BROWSE_TOTAL, 20_000)),
    );
    await exportLogsArchive(response.data || [], "filtered", false);
  });
};
const exportLogsArchive = async (
  rows: CallMonitorItem[],
  scope: "page" | "selected" | "filtered",
  manageLoading = true,
) => {
  if (rows.length === 0) {
    ElMessage.warning($t("operations.monitoring.messages.noExport"));
    return;
  }
  if (manageLoading) {
    exporting.value = true;
  }
  try {
    const encoder = new TextEncoder();
    const files = rows.map((row, index) => ({
      name: buildLogFileName(row, index),
      content: encoder.encode(
        `${JSON.stringify(buildDetailPayload(row.detail), null, 2)}\n`,
      ),
    }));
    const archive = await gzipBytes(createTarArchive(files));
    downloadBinary(buildLogArchiveName(scope), archive, "application/gzip");
  } catch (error) {
    logger.error("Failed to export call monitor archive:", error);
    ElMessage.error($t("operations.monitoring.messages.exportFailed"));
  } finally {
    if (manageLoading) {
      exporting.value = false;
    }
  }
};
const formatDetailValue = (value: unknown) => {
  if (value === null || value === undefined) return "-";
  if (typeof value === "string") {
    return formatUrlDisplay(value);
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
};
const DETAIL_VALUE_MAX_LENGTH = 120;
const formatDetailEntryValue = (value: unknown) => {
  const text = formatDetailValue(value);
  return {
    full: text,
    display: truncateDetailValue(text),
    truncated: text.length > DETAIL_VALUE_MAX_LENGTH,
  };
};
const truncateDetailValue = (value: string) => {
  if (value.length <= DETAIL_VALUE_MAX_LENGTH) {
    return value;
  }
  return `${value.slice(0, DETAIL_VALUE_MAX_LENGTH)}...`;
};
const buildSafeFileName = (value: string) =>
  value.replace(/[^a-zA-Z0-9._-]+/g, "_");
const buildLogFileName = (row: CallMonitorItem, index: number) =>
  `${props.mode}-call-monitor-${buildSafeFileName(row.serial_no || `row-${index + 1}`)}.json`;
const buildLogArchiveName = (scope: "page" | "selected" | "filtered") =>
  `${props.mode}-call-monitor-${scope}-${Date.now().toString(36)}.tar.gz`;
const statusTagType = (statusCode: number) => {
  if (statusCode >= 200 && statusCode < 300) return "success";
  if (statusCode >= 400) return "danger";
  if (statusCode >= 300) return "warning";
  return "info";
};
let refreshTimer: ReturnType<typeof setInterval> | null = null;
let countdownTimer: ReturnType<typeof setInterval> | null = null;

const shouldAutoRefresh = () => {
  if (detailVisible.value || loading.value || exporting.value) return false;
  return canScheduleAutoRefresh();
};

onMounted(() => {
  loadData();
  countdownTimer = setInterval(() => {
    tickRefreshCountdown();
  }, 1000);
  refreshTimer = setInterval(() => {
    if (shouldAutoRefresh()) {
      void loadData({ silent: true });
    } else {
      nextRefreshAtMs.value = null;
      refreshInSec.value = 0;
    }
  }, CALL_MONITOR_LATEST_REFRESH_MS);
});

onUnmounted(() => {
  if (refreshTimer) {
    clearInterval(refreshTimer);
    refreshTimer = null;
  }
  if (countdownTimer) {
    clearInterval(countdownTimer);
    countdownTimer = null;
  }
});
</script>

<style scoped lang="scss">
.detail-drawer-body {
  padding-right: 8px;
}

.detail-toolbar {
  display: flex;
  justify-content: flex-end;
  margin-bottom: 16px;
}

.detail-card {
  margin-bottom: 16px;
}

.detail-card--compact :deep(.el-card__body) {
  padding-top: 12px;
}

.detail-value {
  white-space: pre-wrap;
  word-break: break-word;
  display: inline-block;
  max-width: 100%;
}

.serial-no-cell {
  display: inline-block;
  max-width: 100%;
  white-space: nowrap;
  font-variant-numeric: tabular-nums;
  letter-spacing: 0;
}

.channel-name-cell {
  display: inline-block;
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  vertical-align: bottom;
  cursor: default;
}


:deep(.el-table .cell) {
  .serial-no-cell {
    white-space: nowrap;
  }
}
</style>


<style>
.call-monitor-channel-tooltip {
  max-width: 360px;
  white-space: pre-line;
  line-height: 1.45;
}


.call-monitor-tooltip-instant-enter-active,
.call-monitor-tooltip-instant-leave-active {
  transition: none !important;
}

.call-monitor-tooltip-instant-enter-from,
.call-monitor-tooltip-instant-leave-to {
  opacity: 1 !important;
}
</style>
