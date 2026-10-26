<template>
  <ControlPanelPageShell
    :title="$t('auditLogs.page.title')"
    icon="ri:file-history-line"
  >
    <ResourceTableShell
      v-model:columns="columns"
      :title="$t('auditLogs.list.title')"
      :loading="loading"
      :data="tableData"
      :pagination="artPagination"
      :empty-text="emptyDescription"
      stripe
      border
      @refresh="loadData"
      @selection-change="handleSelectionChange"
      @pagination:size-change="handlePageSizeChange"
      @pagination:current-change="handlePageCurrentChange"
    >
      <template #actions>
        <ListQueryBar
          v-model:q="searchForm.q"
          v-model:filters="listFilters"
          :filter-catalog="auditLogsCatalog"
          :panel-width="260"
          @search="handleSearch"
        />
        <TableHeaderDatePickerPopover
          v-model="timeRange"
          type="datetimerange"
          :start-placeholder="$t('common.startTime')"
          :end-placeholder="$t('common.endTime')"
          value-format="x"
          @change="handleSearch"
        />
        <TableExportDropdown
          :items="exportItems"
          :label="$t('common.actions.export')"
          @command="handleExport"
        />
        <ElButton @click="handleResetFilters">{{
          $t("common.reset")
        }}</ElButton>
      </template>

      <template #timestamp="{ row }">
        {{ formatDate(row.timestamp) }}
      </template>
      <template #actor_name="{ row }">
        {{ formatActorName(row) }}
      </template>
      <template #action="{ row }">
        <ElTag size="small">{{ formatAction(row.action) }}</ElTag>
      </template>
      <template #resource_type="{ row }">
        {{ formatResourceType(row.resource_type) }}
      </template>
      <template #status="{ row }">
        <StatusIndicator :type="statusTagType(row.status)">
          {{ $t(statusLabelKey(row.status)) }}
        </StatusIndicator>
      </template>
      <template #service="{ row }">
        <ElTag size="small" type="info">{{ formatService(row.service) }}</ElTag>
      </template>
      <template #failure_reason="{ row }">
        {{ formatFailureReason(row.failure_reason) }}
      </template>
      <template #ip_address="{ row }">
        {{ formatIpAddress(row.ip_address) }}
      </template>
      <template #details="{ row }">
        <CommonInlineRow gap="8px">
          <ElButton link type="primary" @click="showDetails(row)">{{
            $t("common.actions.details")
          }}</ElButton>
        </CommonInlineRow>
      </template>
    </ResourceTableShell>

    <CenteredDialog
      v-model="detailVisible"
      :title="$t('auditLogs.detail.title')"
      width="600px"
    >
      <ElDescriptions :column="2" border>
        <ElDescriptionsItem :label="$t('auditLogs.detail.fields.timestamp')">{{
          formatDate(currentLog?.timestamp || "")
        }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('auditLogs.detail.fields.actorName')">{{
          currentLog ? formatActorName(currentLog) : "-"
        }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('auditLogs.detail.fields.actorRole')">{{
          currentLog?.actor_role || "-"
        }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('auditLogs.detail.fields.action')">{{
          formatAction(currentLog?.action || "")
        }}</ElDescriptionsItem>
        <ElDescriptionsItem
          :label="$t('auditLogs.detail.fields.resourceType')"
          >{{
            formatResourceType(currentLog?.resource_type || "")
          }}</ElDescriptionsItem
        >
        <ElDescriptionsItem :label="$t('auditLogs.detail.fields.resourceId')">{{
          currentLog?.resource_id || "-"
        }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('auditLogs.detail.fields.status')">
          <ElTag :type="statusTagType(currentLog?.status || '')">
            {{ $t(statusLabelKey(currentLog?.status || "")) }}
          </ElTag>
        </ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('auditLogs.detail.fields.ipAddress')">{{
          formatIpAddress(currentLog?.ip_address)
        }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('auditLogs.detail.fields.service')">{{
          formatService(currentLog?.service)
        }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('auditLogs.detail.fields.requestId')">{{
          currentLog?.request_id || "-"
        }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('auditLogs.detail.fields.method')">{{
          currentLog?.method || "-"
        }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('auditLogs.detail.fields.path')">{{
          currentLog?.path || "-"
        }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('auditLogs.detail.fields.statusCode')">{{
          currentLog?.status_code ?? "-"
        }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('auditLogs.detail.fields.durationMs')">{{
          currentLog?.duration_ms ?? "-"
        }}</ElDescriptionsItem>
        <ElDescriptionsItem
          :label="$t('auditLogs.detail.fields.failureReason')"
          >{{
            formatFailureReason(currentLog?.failure_reason)
          }}</ElDescriptionsItem
        >
        <ElDescriptionsItem
          :label="$t('auditLogs.detail.fields.userAgent')"
          :span="2"
          >{{ currentLog?.user_agent || "-" }}</ElDescriptionsItem
        >
        <ElDescriptionsItem
          v-if="currentLog?.request_summary"
          :label="$t('auditLogs.detail.fields.requestSummary')"
          :span="2"
          >{{
            formatDetailValue(currentLog?.request_summary)
          }}</ElDescriptionsItem
        >
        <ElDescriptionsItem
          v-if="currentLog?.error_message"
          :label="$t('auditLogs.detail.fields.errorMessage')"
          :span="2"
        >
          <ElAlert type="error">{{ currentLog?.error_message }}</ElAlert>
        </ElDescriptionsItem>
      </ElDescriptions>
      <div v-if="auditDetailRows.length" class="mt-4">
        <h4 class="mb-2">{{ $t("auditLogs.detail.moreTitle") }}</h4>
        <ElDescriptions :column="1" border>
          <ElDescriptionsItem
            v-for="item in auditDetailRows"
            :key="item.label"
            :label="item.label"
          >
            {{ item.value }}
          </ElDescriptionsItem>
        </ElDescriptions>
      </div>
      <div v-if="changeRows.length" class="mt-4">
        <h4 class="mb-2">{{ $t("auditLogs.detail.changesTitle") }}</h4>
        <ElTable :data="changeRows" border size="small">
          <ElTableColumn
            prop="label"
            :label="$t('auditLogs.detail.changesField')"
            min-width="120"
          />
          <ElTableColumn
            :label="$t('auditLogs.detail.changesBefore')"
            min-width="160"
          >
            <template #default="{ row }">
              <span :class="{ 'audit-changed': row.changed }">{{
                row.before
              }}</span>
            </template>
          </ElTableColumn>
          <ElTableColumn
            :label="$t('auditLogs.detail.changesAfter')"
            min-width="160"
          >
            <template #default="{ row }">
              <span :class="{ 'audit-changed': row.changed }">{{
                row.after
              }}</span>
            </template>
          </ElTableColumn>
        </ElTable>
      </div>
    </CenteredDialog>
  </ControlPanelPageShell>
</template>

<script setup lang="ts">
import { ref } from "vue";
import type { ListFilter } from "@/types/list-query";
import { auditLogsCatalog } from "@/list-catalog/common-resources";
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import ListQueryBar from "@/components/common/list-query-bar/index.vue";
import TableHeaderDatePickerPopover from "@/components/common/table-header-date-picker-popover/index.vue";
import CenteredDialog from "@/components/common/CenteredDialog.vue";
import CommonInlineRow from "@/components/common/inline-row/index.vue";
import ResourceTableShell from "@/components/common/resource-table-shell/index.vue";
import StatusIndicator from "@/components/common/StatusIndicator.vue";
import TableExportDropdown from "@/components/table/TableExportDropdown.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { exportResourceCsv } from "@/composables/useResourceCsvImportExport";
import type {
  AuditLog,
  AuditLogListQuery,
  AuditLogDetail,
} from "@/types/api/audit";
import {
  fetchAuditLogList,
  fetchAuditLogById,
  exportAuditLogs,
} from "@/api/audit";
import { logger } from "@/utils/sys";
import { formatDateTime as formatDate } from "@/utils/format";
import { downloadCsv } from "@/utils/csv";
import { stripUrlScheme } from "@/utils/sensitive";
import { $t } from "@/locales";
import {
  ACTION_LABELS,
  FAILURE_REASON_LABELS,
  RESOURCE_TYPE_LABELS,
  SERVICE_LABELS,
  diffChanges,
  guardCsvFormula,
  statusLabelKey,
  statusTagType,
} from "./labels";
import type { ColumnOption, PaginationConfig } from "@/types/component";
defineOptions({ name: "AuditLogs" });
type ExportCommand = "all" | "selected";
type AuditLogCsvRow = Record<string, string>;
const loading = ref(false);
const tableData = ref<AuditLog[]>([]);
const selectedRows = ref<AuditLog[]>([]);
const detailVisible = ref(false);
const currentLog = ref<AuditLogDetail | null>(null);
const ACTION_VERB_LABELS: Record<string, string> = {
  login: "auditLogs.verbs.login",
  logout: "auditLogs.verbs.logout",
  create: "auditLogs.verbs.create",
  update: "auditLogs.verbs.update",
  delete: "auditLogs.verbs.delete",
  revoke: "auditLogs.verbs.revoke",
  grant: "auditLogs.verbs.grant",
  renew: "auditLogs.verbs.renew",
  issue: "auditLogs.verbs.issue",
  suspend: "auditLogs.verbs.suspend",
  activate: "auditLogs.verbs.activate",
  recharge: "auditLogs.verbs.recharge",
  refund: "auditLogs.verbs.refund",
  rollback: "auditLogs.verbs.rollback",
  export: "auditLogs.verbs.export",
  import: "auditLogs.verbs.import",
  change: "auditLogs.verbs.change",
  reset: "auditLogs.verbs.reset",
};
const formatService = (service?: string | null) => {
  if (!service) {
    return "-";
  }
  return SERVICE_LABELS[service] ? $t(SERVICE_LABELS[service]) : service;
};
const formatResourceType = (type: string) => {
  if (!type) {
    return "-";
  }
  return RESOURCE_TYPE_LABELS[type]
    ? $t(RESOURCE_TYPE_LABELS[type])
    : type.replaceAll("_", " ");
};
const formatAction = (action: string) => {
  if (!action) {
    return "-";
  }
  if (ACTION_LABELS[action]) {
    return $t(ACTION_LABELS[action]);
  }
  const [resourceType, verb] = action.split(".");
  const resourceLabel = formatResourceType(resourceType);
  const verbLabel = ACTION_VERB_LABELS[verb]
    ? $t(ACTION_VERB_LABELS[verb])
    : "";
  if (resourceType && verbLabel) {
    return `${verbLabel}${resourceLabel}`;
  }
  return action.replaceAll(".", " / ").replaceAll("_", " ");
};
const timeRange = ref<[number, number] | null>(null);
const listFilters = ref<ListFilter[]>([]);
const searchForm = reactive({
  q: "",
});
const hasActiveSearch = computed(
  () =>
    Boolean(searchForm.q?.trim()) ||
    listFilters.value.length > 0 ||
    Boolean(timeRange.value?.length === 2),
);
const emptyDescription = computed(() =>
  hasActiveSearch.value
    ? $t("auditLogs.empty.filtered")
    : $t("auditLogs.empty.default"),
);
const DETAIL_LABELS: Record<string, string> = {
  reason: "auditLogs.detailLabels.reason",
  operator: "auditLogs.detailLabels.operator",
  operator_id: "auditLogs.detailLabels.operatorId",
  ip_address: "auditLogs.detailLabels.ipAddress",
  selected_channel_id: "auditLogs.detailLabels.selectedChannelId",
  route_rule_id: "auditLogs.detailLabels.routeRuleId",
  channel_id: "auditLogs.detailLabels.channelId",
  binding_id: "auditLogs.detailLabels.bindingId",
  old_route: "auditLogs.detailLabels.oldRoute",
  new_route: "auditLogs.detailLabels.newRoute",
  route_rule: "auditLogs.detailLabels.routeRule",
  bindings: "auditLogs.detailLabels.bindings",
  route: "auditLogs.detailLabels.route",
  message: "auditLogs.detailLabels.message",
  status: "auditLogs.detailLabels.status",
  name: "auditLogs.detailLabels.name",
  id: "auditLogs.detailLabels.id",
  created_at: "auditLogs.detailLabels.createdAt",
  updated_at: "auditLogs.detailLabels.updatedAt",
  expires_at: "auditLogs.detailLabels.expiresAt",
  product_id: "auditLogs.detailLabels.productId",
  tenant_id: "auditLogs.detailLabels.tenantId",
  tenant_code: "auditLogs.detailLabels.tenantCode",
  priority: "auditLogs.detailLabels.priority",
  metadata: "auditLogs.detailLabels.metadata",
  switch_condition: "auditLogs.detailLabels.switchCondition",
  channel_order: "auditLogs.detailLabels.channelOrder",
  description: "auditLogs.detailLabels.description",
  enabled: "auditLogs.detailLabels.enabled",
  request: "auditLogs.detailLabels.request",
  product: "auditLogs.detailLabels.product",
  reservation: "auditLogs.detailLabels.reservation",
  reservation_id: "auditLogs.detailLabels.reservationId",
  active_reservation_id: "auditLogs.detailLabels.activeReservationId",
  reserved_by: "auditLogs.detailLabels.reservedBy",
  reserved_at: "auditLogs.detailLabels.reservedAt",
  upstream_url: "auditLogs.detailLabels.upstreamUrl",
  timeout_secs: "auditLogs.detailLabels.timeoutSecs",
  bill_id: "auditLogs.detailLabels.billId",
  bill_kind: "auditLogs.detailLabels.billKind",
  transaction_id: "auditLogs.detailLabels.transactionId",
  amount: "auditLogs.detailLabels.amount",
  direction: "auditLogs.detailLabels.direction",
  ip_addresses: "auditLogs.detailLabels.ipAddresses",
  total_added: "auditLogs.detailLabels.totalAdded",
  total_errors: "auditLogs.detailLabels.totalErrors",
  algorithm: "auditLogs.detailLabels.algorithm",
  max_daily: "auditLogs.detailLabels.maxDaily",
  max_concurrent: "auditLogs.detailLabels.maxConcurrent",
  max_bandwidth_mbps: "auditLogs.detailLabels.maxBandwidthMbps",
  api_limits: "auditLogs.detailLabels.apiLimits",
  burst_size: "auditLogs.detailLabels.burstSize",
  window_duration_secs: "auditLogs.detailLabels.windowDurationSecs",
  expected_updated_at: "auditLogs.detailLabels.expectedUpdatedAt",
  default_ttl: "auditLogs.detailLabels.defaultTtl",
  max_cache_size: "auditLogs.detailLabels.maxCacheSize",
  api_ttl: "auditLogs.detailLabels.apiTtl",
  tenant_isolated: "auditLogs.detailLabels.tenantIsolated",
  warmup_enabled: "auditLogs.detailLabels.warmupEnabled",
  warmup_keys: "auditLogs.detailLabels.warmupKeys",
  scope: "auditLogs.detailLabels.scope",
  changes: "auditLogs.detailLabels.changes",
  site_name: "auditLogs.detailLabels.siteName",
  site_url: "auditLogs.detailLabels.siteUrl",
  tenant_gateway_url: "auditLogs.detailLabels.tenantGatewayUrl",
  enable_notifications: "auditLogs.detailLabels.enableNotifications",
  enable_audit_log: "auditLogs.detailLabels.enableAuditLog",
  session_timeout: "auditLogs.detailLabels.sessionTimeout",
  max_upload_size: "auditLogs.detailLabels.maxUploadSize",
  username: "auditLogs.detailLabels.username",
  first_login: "auditLogs.detailLabels.firstLogin",
  language: "auditLogs.detailLabels.language",
  email: "auditLogs.detailLabels.email",
  encryption_mode: "auditLogs.detailLabels.encryptionMode",
  require_signature: "auditLogs.detailLabels.requireSignature",
  nonce_ttl_seconds: "auditLogs.detailLabels.nonceTtlSeconds",
  clock_skew_seconds: "auditLogs.detailLabels.clockSkewSeconds",
  current_key_version: "auditLogs.detailLabels.currentKeyVersion",
  key_version: "auditLogs.detailLabels.keyVersion",
  mode: "auditLogs.detailLabels.mode",
  manifest: "auditLogs.detailLabels.manifest",
  checked: "auditLogs.detailLabels.checked",
  verified: "auditLogs.detailLabels.verified",
  failed_count: "auditLogs.detailLabels.failedCount",
  path: "auditLogs.detailLabels.path",
  operation: "auditLogs.detailLabels.operation",
  authorization_type: "auditLogs.detailLabels.authorizationType",
  activate_now: "auditLogs.detailLabels.activateNow",
  delivery_log_id: "auditLogs.detailLabels.deliveryLogId",
};
const pagination = reactive({
  page: 1,
  limit: 10,
  total: 0,
});
const columns = ref<ColumnOption<AuditLog>[]>([
  { type: "selection", width: 55 },
  {
    prop: "timestamp",
    label: $t("auditLogs.columns.timestamp"),
    width: 180,
    useSlot: true,
  },
  {
    prop: "actor_name",
    label: $t("auditLogs.columns.actorName"),
    minWidth: 100,
    useSlot: true,
  },
  {
    prop: "action",
    label: $t("auditLogs.columns.action"),
    minWidth: 120,
    useSlot: true,
  },
  {
    prop: "resource_type",
    label: $t("auditLogs.columns.resourceType"),
    minWidth: 96,
    useSlot: true,
  },
  {
    prop: "status",
    label: $t("auditLogs.columns.status"),
    width: 80,
    useSlot: true,
  },
  {
    prop: "service",
    label: $t("auditLogs.columns.service"),
    width: 104,
    useSlot: true,
  },
  {
    prop: "failure_reason",
    label: $t("auditLogs.columns.failureReason"),
    minWidth: 112,
    useSlot: true,
  },
  {
    prop: "ip_address",
    label: $t("auditLogs.columns.ipAddress"),
    minWidth: 112,
    useSlot: true,
  },
  {
    prop: "details",
    label: $t("auditLogs.columns.details"),
    width: 88,
    align: "center",
    fixed: "right",
    useSlot: true,
  },
]);
const artPagination = computed<PaginationConfig>(() => ({
  current: pagination.page,
  size: pagination.limit,
  total: pagination.total,
}));
const exportItems = computed(() => [
  {
    command: "all",
    label: $t("common.actions.exportAll"),
    disabled: tableData.value.length === 0,
  },
  {
    command: "selected",
    label: $t("common.actions.exportSelected", {
      count: selectedRows.value.length,
    }),
    disabled: selectedRows.value.length === 0,
  },
]);
const formatIpAddress = (ip?: string) => {
  return ip && ip.trim() ? ip : $t("auditLogs.detail.notRecorded");
};
const getRecord = (value: unknown): Record<string, unknown> => {
  if (!value || typeof value !== "object" || Array.isArray(value)) return {};
  return value as Record<string, unknown>;
};
const formatDetailLabel = (key: string) => {
  return DETAIL_LABELS[key] ? $t(DETAIL_LABELS[key]) : key.replaceAll("_", " ");
};
const SENSITIVE_DETAIL_KEYS = new Set(["endpoint_url", "upstream_url"]);
const formatDetailValue = (value: unknown): string => {
  if (value === null || value === undefined || value === "") return "-";
  if (typeof value === "boolean")
    return value ? $t("auditLogs.values.yes") : $t("auditLogs.values.no");
  if (typeof value === "number") return String(value);
  if (typeof value === "string") return stripUrlScheme(value);
  if (Array.isArray(value)) {
    if (!value.length) return $t("auditLogs.values.none");
    return value.map(formatDetailValue).join("、");
  }
  const record = getRecord(value);
  const routeConfig = getRecord(record.route_config);
  if (Object.keys(routeConfig).length) {
    return formatRouteConfig(routeConfig);
  }
  if ("max_retries" in record || "timeout_ms" in record) {
    return formatSwitchCondition(record);
  }
  if ("route_rule" in record || "bindings" in record) {
    return formatRouteSnapshot(record);
  }
  const summary = Object.entries(record)
    .filter(([, item]) => item !== null && item !== undefined && item !== "")
    .slice(0, 6)
    .map(([key, item]) => {
      const value =
        SENSITIVE_DETAIL_KEYS.has(key) && typeof item === "string"
          ? stripUrlScheme(item)
          : formatDetailValue(item);
      return `${formatDetailLabel(key)}：${value}`;
    });
  return summary.length ? summary.join("；") : "-";
};
const formatRouteConfig = (config: Record<string, unknown>) => {
  const lines = [
    config.purpose_label
      ? `${$t("auditLogs.routeConfig.purpose")}${formatDetailValue(config.purpose_label)}`
      : "",
    config.change_reason
      ? `${$t("auditLogs.routeConfig.changeReason")}${formatDetailValue(config.change_reason)}`
      : "",
    Array.isArray(config.tags) && config.tags.length
      ? `${$t("auditLogs.routeConfig.tags")}${formatDetailValue(config.tags)}`
      : "",
    config.effective_mode === "window"
      ? `${$t("auditLogs.routeConfig.effectiveWindow")}${formatDetailValue(config.effective_start_at)} ${$t("auditLogs.routeConfig.to")} ${formatDetailValue(config.effective_end_at)}`
      : $t("auditLogs.routeConfig.effectiveNow"),
    config.description
      ? `${$t("auditLogs.routeConfig.description")}${formatDetailValue(config.description)}`
      : "",
  ].filter(Boolean);
  return lines.join("；");
};
const formatSwitchCondition = (condition: Record<string, unknown>) => {
  const timeoutMs =
    typeof condition.timeout_ms === "number" ? condition.timeout_ms : 3000;
  const maxRetries =
    typeof condition.max_retries === "number" ? condition.max_retries : 1;
  const failoverEnabled = condition.failover_enabled !== false;
  const failureTypes = Array.isArray(condition.failure_types)
    ? condition.failure_types.map(formatDetailValue).join("、")
    : $t("auditLogs.routeConfig.defaultFailureTypes");
  return $t("auditLogs.routeConfig.switchCondition", {
    timeoutMs,
    maxRetries,
    failureTypes,
    failoverText: failoverEnabled
      ? $t("auditLogs.routeConfig.switchOnFailure")
      : $t("auditLogs.routeConfig.noSwitch"),
  });
};
const formatRouteSnapshot = (snapshot: Record<string, unknown>) => {
  const routeRule = getRecord(snapshot.route_rule);
  const bindings = Array.isArray(snapshot.bindings) ? snapshot.bindings : [];
  const firstBinding = getRecord(bindings[0]);
  const parts = [
    routeRule.product_id
      ? `${$t("auditLogs.routeSnapshot.productId")}${formatDetailValue(routeRule.product_id)}`
      : "",
    routeRule.tenant_id
      ? `${$t("auditLogs.routeSnapshot.tenantId")}${formatDetailValue(routeRule.tenant_id)}`
      : $t("auditLogs.routeSnapshot.defaultRoute"),
    routeRule.priority
      ? `${$t("auditLogs.routeSnapshot.priority")}${formatDetailValue(routeRule.priority)}`
      : "",
    routeRule.status
      ? `${$t("auditLogs.routeSnapshot.status")}${formatDetailValue(routeRule.status)}`
      : "",
    bindings.length
      ? `${$t("auditLogs.routeSnapshot.candidateCount")}${bindings.length}`
      : "",
    firstBinding.channel_id
      ? `${$t("auditLogs.routeSnapshot.firstChannel")}${formatDetailValue(firstBinding.channel_id)}`
      : "",
  ].filter(Boolean);
  return parts.join("；");
};
const formatActorName = (log: AuditLog) => {
  if (log.actor_name) return log.actor_name;
  return log.actor_type === "anonymous"
    ? $t("auditLogs.values.anonymous")
    : "-";
};
const formatFailureReason = (reason?: string | null) => {
  if (!reason) return "-";
  return FAILURE_REASON_LABELS[reason]
    ? $t(FAILURE_REASON_LABELS[reason])
    : reason.replaceAll("_", " ");
};
const changeRows = computed(() => {
  const changes = currentLog.value?.changes;
  if (!changes) return [];
  return diffChanges(changes.before, changes.after).map((row) => ({
    label: formatDetailLabel(row.key),
    before: formatDetailValue(row.before),
    after: formatDetailValue(row.after),
    changed: row.changed,
  }));
});
const auditDetailRows = computed(() => {
  const details = currentLog.value?.details || {};
  return Object.entries(details)
    .filter(
      ([, value]) => value !== null && value !== undefined && value !== "",
    )
    .map(([key, value]) => ({
      label: formatDetailLabel(key),
      value: formatDetailValue(value),
    }));
});
const buildQueryParams = (includePagination = true) => {
  const keyword = searchForm.q?.trim() || "";
  const params: AuditLogListQuery = {};
  if (includePagination) {
    params.page = pagination.page;
    params.limit = pagination.limit;
  }
  if (keyword) {
    params.q = keyword;
  }
  if (listFilters.value.length) {
    params.filters = JSON.stringify(listFilters.value);
  }
  if (timeRange.value?.length === 2) {
    params.start_time = Math.floor(Number(timeRange.value[0]) / 1000);
    params.end_time = Math.floor(Number(timeRange.value[1]) / 1000);
  }
  return params;
};
const loadData = async () => {
  const params = buildQueryParams();
  await runWithLoading(loading, async () => {
    const res = await fetchAuditLogList(params);
    tableData.value = res.logs;
    pagination.total = res.total;
    selectedRows.value = [];
  }).catch((error) => {
    logger.error("Failed to load audit logs:", error);
  });
};
const handleSearch = () => {
  pagination.page = 1;
  loadData();
};

const handleResetFilters = () => {
  searchForm.q = "";
  listFilters.value = [];
  timeRange.value = null;
  pagination.page = 1;
  loadData();
};
const handlePageSizeChange = (size: number) => {
  pagination.limit = size;
  pagination.page = 1;
  loadData();
};
const handlePageCurrentChange = (page: number) => {
  pagination.page = page;
  loadData();
};
const handleSelectionChange = (selection: AuditLog[]) => {
  selectedRows.value = selection;
};
const showDetails = async (row: AuditLog) => {
  try {
    currentLog.value = await fetchAuditLogById(row.id);
    detailVisible.value = true;
  } catch (error) {
    logger.error("Failed to load audit log detail:", error);
  }
};
const handleExport = async (command: string) => {
  try {
    const exportCommand = command as ExportCommand;
    if (exportCommand === "selected") {
      const rows = selectedRows.value;
      const csvRows: AuditLogCsvRow[] = rows.map((row) => {
        const record: AuditLogCsvRow = {
          timestamp: formatDate(row.timestamp),
          actor_name: formatActorName(row),
          actor_role: row.actor_role || "-",
          action: formatAction(row.action),
          resource_type: formatResourceType(row.resource_type),
          status: $t(statusLabelKey(row.status)),
          failure_reason: formatFailureReason(row.failure_reason),
          ip_address: formatIpAddress(row.ip_address),
          request_id: row.request_id || "-",
          route: [row.method, row.path].filter(Boolean).join(" ") || "-",
          status_code:
            row.status_code === null || row.status_code === undefined
              ? "-"
              : String(row.status_code),
          duration_ms:
            row.duration_ms === null || row.duration_ms === undefined
              ? "-"
              : String(row.duration_ms),
          service: formatService(row.service),
          details: formatDetailValue(row.details),
        };
        return Object.fromEntries(
          Object.entries(record).map(([key, value]) => [
            key,
            guardCsvFormula(value),
          ]),
        );
      });
      const filename = `audit_logs_selected_${new Date().toISOString().slice(0, 19).replaceAll(":", "-")}.csv`;
      exportResourceCsv(
        csvRows,
        [
          { key: "timestamp", label: $t("auditLogs.columns.timestamp") },
          { key: "actor_name", label: $t("auditLogs.columns.actorName") },
          { key: "actor_role", label: $t("auditLogs.columns.actorRole") },
          { key: "action", label: $t("auditLogs.columns.action") },
          { key: "resource_type", label: $t("auditLogs.columns.resourceType") },
          { key: "status", label: $t("auditLogs.columns.status") },
          {
            key: "failure_reason",
            label: $t("auditLogs.columns.failureReason"),
          },
          { key: "ip_address", label: $t("auditLogs.columns.ipAddress") },
          { key: "request_id", label: $t("auditLogs.columns.requestId") },
          { key: "route", label: $t("auditLogs.columns.route") },
          { key: "status_code", label: $t("auditLogs.columns.statusCode") },
          { key: "duration_ms", label: $t("auditLogs.columns.durationMs") },
          { key: "service", label: $t("auditLogs.columns.service") },
          { key: "details", label: $t("auditLogs.columns.details") },
        ],
        filename,
      );
      return;
    }
    const params = buildQueryParams(false);
    const csv = await exportAuditLogs(params);
    const filename = `audit_logs_all_${new Date()
      .toISOString()
      .slice(0, 19)
      .replaceAll(":", "-")}.csv`;
    downloadCsv(csv.startsWith("\uFEFF") ? csv : `\uFEFF${csv}`, filename);
  } catch (error) {
    logger.error("Failed to export audit logs:", error);
  }
};
onMounted(() => {
  loadData();
});
</script>

<style scoped>
.audit-changed {
  font-weight: 600;
  color: var(--el-color-warning);
}
</style>
