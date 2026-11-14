<template>
  <ControlPanelPageShell
    :title="$t('auditLogs.page.title')"
    icon="ri:file-history-line"
  >
    <ResourceTableShell
      v-model:columns="columns"
      :loading="loading"
      :data="displayTableData"
      :pagination="artPagination"
      :empty-text="emptyDescription"
      stripe
      @refresh="loadData"
      @selection-change="handleSelectionChange"
      @pagination:size-change="handlePageSizeChange"
      @pagination:current-change="handlePageCurrentChange"
    >
      <template #actions>
        <ListQueryBar
          v-model:q="searchForm.q"
          v-model:filters="listFilters"
          :catalog="auditLogsCatalog"
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
        <ElCheckbox
          v-model="searchForm.includeSelfAudit"
          class="audit-self-toggle"
          @change="handleSearch"
        >
          {{ $t("auditLogs.filters.includeSelfAudit") }}
        </ElCheckbox>
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
        <ElTooltip
          v-if="row.chainServices.length >= 2 && row.request_id"
          :content="chainTooltipText(row)"
          placement="top"
        >
          <button
            type="button"
            class="audit-chain-badge"
            :aria-expanded="row.chainExpanded"
            @click.stop="toggleChainExpand(row.request_id)"
          >
            <span class="audit-chain-badge__code">{{
              formatChainFlowCode(row.chainServices)
            }}</span>
            <ArtSvgIcon
              :icon="
                row.chainExpanded
                  ? 'ri:arrow-up-s-line'
                  : 'ri:arrow-down-s-line'
              "
              class="audit-chain-badge__icon"
            />
          </button>
        </ElTooltip>
        <span v-else-if="row.isMirrorChild" class="audit-mirror-cell">
          <span class="audit-mirror-prefix">{{
            $t("auditLogs.chain.mirrorPrefix")
          }}</span>
          <ElTag size="small" type="info">{{
            formatService(row.service)
          }}</ElTag>
        </span>
        <ElTag v-else size="small" type="info">{{
          formatService(row.service)
        }}</ElTag>
      </template>
      <template #failure_reason="{ row }">
        {{ formatFailureReason(row.failure_reason) }}
      </template>
      <template #ip_address="{ row }">
        <CopyableText
          v-if="row.ip_address?.trim()"
          :value="row.ip_address"
          :label="$t('auditLogs.detail.fields.ipAddress')"
        />
        <span v-else>{{ formatIpAddress(row.ip_address) }}</span>
      </template>
      <template #details="{ row }">
        <CommonInlineRow gap="8px">
          <ElButton link type="primary" @click="showDetails(row)">{{
            $t("common.actions.details")
          }}</ElButton>
        </CommonInlineRow>
      </template>
    </ResourceTableShell>

    <AdaptiveDrawer
      v-model="detailVisible"
      size="760px"
      destroy-on-close
      @closed="handleDrawerClosed"
    >
      <template #header>
        <CommonDrawerHeader
          :title="drawerActionTitle"
          :subtitle="currentLog?.request_id || ''"
        />
      </template>

      <div v-loading="detailLoading" class="audit-detail-body">
        <div
          v-if="relatedChainRecords.length > 1"
          class="audit-related-bar mb-3"
        >
          <span class="audit-related-bar__label">
            {{ $t("auditLogs.chain.detailTitle") }}
          </span>
          <div class="audit-related-bar__items">
            <ElButton
              v-for="item in relatedChainRecords"
              :key="item.id"
              size="small"
              :type="item.id === currentLog?.id ? 'primary' : 'default'"
              :plain="item.id !== currentLog?.id"
              @click="showDetails(item)"
            >
              {{ formatService(item.service) }} ·
              {{ formatAction(item.action) }}
            </ElButton>
          </div>
        </div>

        <ElDescriptions
          :column="2"
          border
          :label-width="detailLabelWidth"
          class="audit-main-descriptions"
        >
          <ElDescriptionsItem
            :label="$t('auditLogs.detail.fields.timestamp')"
            ><span class="audit-nowrap-val">{{
              formatDate(currentLog?.timestamp || "")
            }}</span></ElDescriptionsItem
          >
          <ElDescriptionsItem :label="$t('auditLogs.detail.fields.status')">
            <ElTag size="small" :type="statusTagType(currentLog?.status || '')">
              {{ $t(statusLabelKey(currentLog?.status || "")) }}
            </ElTag>
          </ElDescriptionsItem>
          <ElDescriptionsItem
            :label="$t('auditLogs.detail.fields.actorName')"
            >{{
              currentLog ? formatActorName(currentLog) : EMPTY_PLACEHOLDER
            }}</ElDescriptionsItem
          >
          <ElDescriptionsItem
            :label="$t('auditLogs.detail.fields.actorRole')"
            >{{ currentLog?.actor_role || EMPTY_PLACEHOLDER }}</ElDescriptionsItem
          >
          <ElDescriptionsItem :label="$t('auditLogs.detail.fields.action')">{{
            formatAction(currentLog?.action || "")
          }}</ElDescriptionsItem>
          <ElDescriptionsItem :label="$t('auditLogs.detail.fields.service')">{{
            formatService(currentLog?.service)
          }}</ElDescriptionsItem>
          <ElDescriptionsItem
            :label="$t('auditLogs.detail.fields.resourceType')"
            >{{
              formatResourceType(currentLog?.resource_type || "")
            }}</ElDescriptionsItem
          >
          <ElDescriptionsItem
            :label="$t('auditLogs.detail.fields.resourceId')"
          >
            <CopyableText
              :value="currentLog?.resource_id"
              :label="$t('auditLogs.detail.fields.resourceId')"
            />
          </ElDescriptionsItem>
          <ElDescriptionsItem :label="$t('auditLogs.detail.fields.ipAddress')">
            <CopyableText
              v-if="currentLog?.ip_address?.trim()"
              :value="currentLog.ip_address"
              :label="$t('auditLogs.detail.fields.ipAddress')"
            />
            <span v-else>{{ formatIpAddress(currentLog?.ip_address) }}</span>
          </ElDescriptionsItem>
          <ElDescriptionsItem :label="$t('auditLogs.detail.fields.requestId')">
            <CopyableText
              :value="currentLog?.request_id"
              :label="$t('auditLogs.detail.fields.requestId')"
            />
          </ElDescriptionsItem>
          <ElDescriptionsItem :label="$t('auditLogs.detail.fields.method')">{{
            currentLog?.method || EMPTY_PLACEHOLDER
          }}</ElDescriptionsItem>
          <ElDescriptionsItem
            :label="$t('auditLogs.detail.fields.statusCode')"
            >{{ currentLog?.status_code ?? EMPTY_PLACEHOLDER }}</ElDescriptionsItem
          >
          <ElDescriptionsItem
            :label="$t('auditLogs.detail.fields.durationMs')"
            >{{
              currentLog?.duration_ms === null ||
              currentLog?.duration_ms === undefined
                ? EMPTY_PLACEHOLDER
                : `${currentLog.duration_ms} ms`
            }}</ElDescriptionsItem
          >
          <ElDescriptionsItem
            :label="$t('auditLogs.detail.fields.failureReason')"
            >{{
              formatFailureReason(currentLog?.failure_reason)
            }}</ElDescriptionsItem
          >
          <ElDescriptionsItem
            :label="$t('auditLogs.detail.fields.path')"
            :span="2"
            >{{ currentLog?.path || EMPTY_PLACEHOLDER }}</ElDescriptionsItem
          >
          <ElDescriptionsItem
            :label="$t('auditLogs.detail.fields.userAgent')"
            :span="2"
            >{{
              currentLog?.user_agent || EMPTY_PLACEHOLDER
            }}</ElDescriptionsItem
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
            <ElAlert type="error" :closable="false">{{
              currentLog?.error_message
            }}</ElAlert>
          </ElDescriptionsItem>
        </ElDescriptions>

        <div v-if="auditDetailRows.length" class="mt-4">
          <h4 class="audit-section-title mb-2">
            {{ $t("auditLogs.detail.moreTitle") }}
          </h4>
          <ElDescriptions
            :column="1"
            border
            :label-width="extDetailLabelWidth"
            class="audit-ext-descriptions"
          >
            <ElDescriptionsItem
              v-for="item in auditDetailRows"
              :key="item.key"
              :label="item.label"
            >
              {{ item.value }}
            </ElDescriptionsItem>
          </ElDescriptions>
        </div>

        <div v-if="changeRows.length" class="mt-4">
          <h4 class="audit-section-title mb-2">
            {{ $t("auditLogs.detail.changesTitle") }}
          </h4>
          <ElTable :data="changeRows" border size="small">
            <ElTableColumn
              prop="label"
              :label="$t('auditLogs.detail.changesField')"
              min-width="140"
            />
            <ElTableColumn
              :label="$t('auditLogs.detail.changesBefore')"
              min-width="200"
            >
              <template #default="{ row }">
                <span :class="{ 'audit-changed': row.changed }">{{
                  row.before
                }}</span>
              </template>
            </ElTableColumn>
            <ElTableColumn
              :label="$t('auditLogs.detail.changesAfter')"
              min-width="200"
            >
              <template #default="{ row }">
                <span :class="{ 'audit-changed': row.changed }">{{
                  row.after
                }}</span>
              </template>
            </ElTableColumn>
          </ElTable>
        </div>

        <ElCollapse
          v-if="hasRawJsonPayload"
          v-model="rawJsonPanels"
          class="audit-raw-collapse mt-4"
        >
          <ElCollapseItem name="raw">
            <template #title>
              <div class="audit-raw-header">
                <span>{{ $t("auditLogs.detail.rawJsonTitle") }}</span>
                <ElButton
                  link
                  type="primary"
                  size="small"
                  @click.stop="handleCopyRawJson"
                >
                  <template #icon
                    ><ArtSvgIcon icon="ri:file-copy-line"
                  /></template>
                  {{ $t("auditLogs.detail.copyRawJson") }}
                </ElButton>
              </div>
            </template>
            <pre class="audit-raw-pre">{{ rawJsonText }}</pre>
          </ElCollapseItem>
        </ElCollapse>
      </div>
    </AdaptiveDrawer>
  </ControlPanelPageShell>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import CopyableText from "@/components/common/copyable-text/index.vue";
import CommonDrawerHeader from "@/components/common/drawer-header/index.vue";
import type { ListFilter } from "@/types/list-query";
import { auditLogsCatalog } from "@/list-catalog/common-resources";
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import ListQueryBar from "@/components/common/list-query-bar/index.vue";
import TableHeaderDatePickerPopover from "@/components/common/table-header-date-picker-popover/index.vue";
import AdaptiveDrawer from "@/components/common/AdaptiveDrawer.vue";
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
import { EMPTY_PLACEHOLDER, formatDateTime as formatDate } from "@/utils/format";
import { downloadCsv } from "@/utils/csv";
import { stripUrlScheme } from "@/utils/sensitive";
import { copyWithFeedback } from "@/utils/ui/copy-feedback";
import { $t } from "@/locales";
import {
  ACTION_LABELS,
  DETAIL_LABELS,
  FAILURE_REASON_LABELS,
  MFA_CATEGORY_LABELS,
  MFA_METHOD_LABELS,
  RESOURCE_TYPE_LABELS,
  SERVICE_LABELS,
  diffChanges,
  formatChainFlowCode,
  groupAuditLogsByRequestId,
  guardCsvFormula,
  isAuditDurationSecondsKey,
  isAuditTimestampKey,
  statusLabelKey,
  statusTagType,
  type GroupedAuditRow,
} from "./labels";
import type { ColumnOption, PaginationConfig } from "@/types/component";

defineOptions({ name: "AuditLogs" });

type ExportCommand = "all" | "selected";
type AuditLogCsvRow = Record<string, string>;

const { t, locale } = useI18n();
const loading = ref(false);
const detailLoading = ref(false);
const tableData = ref<AuditLog[]>([]);
const selectedRows = ref<GroupedAuditRow<AuditLog>[]>([]);
const expandedRequestIds = ref<Set<string>>(new Set());
const detailVisible = ref(false);
const currentLog = ref<AuditLogDetail | null>(null);
const rawJsonPanels = ref<string[]>([]);

const isEnLocale = computed(() => String(locale.value).startsWith("en"));
const detailLabelWidth = computed(() =>
  isEnLocale.value ? "136px" : "116px",
);
const extDetailLabelWidth = computed(() =>
  isEnLocale.value ? "196px" : "148px",
);

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
    return EMPTY_PLACEHOLDER;
  }
  return SERVICE_LABELS[service] ? $t(SERVICE_LABELS[service]) : service;
};

const formatResourceType = (type: string) => {
  if (!type) {
    return EMPTY_PLACEHOLDER;
  }
  return RESOURCE_TYPE_LABELS[type]
    ? $t(RESOURCE_TYPE_LABELS[type])
    : type.replaceAll("_", " ");
};

const formatAction = (action: string) => {
  if (!action) {
    return EMPTY_PLACEHOLDER;
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

const drawerActionTitle = computed(() => {
  if (!currentLog.value) {
    return $t("auditLogs.detail.title");
  }
  return formatAction(currentLog.value.action);
});

const timeRange = ref<[number, number] | null>(null);
const listFilters = ref<ListFilter[]>([]);
const searchForm = reactive({
  q: "",
  includeSelfAudit: false,
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

const pagination = reactive({
  page: 1,
  limit: 10,
  total: 0,
});

const buildAuditColumns = (): ColumnOption<GroupedAuditRow<AuditLog>>[] => {
  const isEn = String(locale.value).startsWith("en");
  return [
    { type: "selection", width: 48 },
    {
      prop: "timestamp",
      label: t("auditLogs.columns.timestamp"),
      minWidth: 160,
      useSlot: true,
    },
    {
      prop: "actor_name",
      label: t("auditLogs.columns.actorName"),
      minWidth: 88,
      useSlot: true,
    },
    {
      prop: "action",
      label: t("auditLogs.columns.action"),
      minWidth: 124,
      useSlot: true,
    },
    {
      prop: "resource_type",
      label: t("auditLogs.columns.resourceType"),
      minWidth: 88,
      useSlot: true,
    },
    {
      prop: "status",
      label: t("auditLogs.columns.status"),
      width: isEn ? 92 : 76,
      useSlot: true,
    },
    {
      prop: "service",
      label: t("auditLogs.columns.service"),
      minWidth: isEn ? 140 : 128,
      useSlot: true,
    },
    {
      prop: "failure_reason",
      label: t("auditLogs.columns.failureReason"),
      minWidth: 96,
      useSlot: true,
    },
    {
      prop: "ip_address",
      label: t("auditLogs.columns.ipAddress"),
      minWidth: 112,
      useSlot: true,
    },
    {
      prop: "details",
      label: t("common.fields.operations"),
      width: isEn ? 88 : 76,
      align: "center",
      fixed: "right",
      useSlot: true,
    },
  ];
};

const columns = ref<ColumnOption<GroupedAuditRow<AuditLog>>[]>(
  buildAuditColumns(),
);
watch(locale, () => {
  const prev = new Map(
    columns.value.map((col) => [String(col.prop ?? col.type ?? ""), col]),
  );
  columns.value = buildAuditColumns().map((col) => {
    const existing = prev.get(String(col.prop ?? col.type ?? ""));
    return existing
      ? {
          ...col,
          visible: existing.visible ?? col.visible,
          checked: existing.checked ?? col.checked,
        }
      : col;
  });
});

const displayTableData = computed<GroupedAuditRow<AuditLog>[]>(() =>
  groupAuditLogsByRequestId(tableData.value, expandedRequestIds.value),
);

const toggleChainExpand = (requestId?: string | null) => {
  const key = requestId?.trim();
  if (!key) return;
  const next = new Set(expandedRequestIds.value);
  if (next.has(key)) {
    next.delete(key);
  } else {
    next.add(key);
  }
  expandedRequestIds.value = next;
};

const chainTooltipText = (row: GroupedAuditRow<AuditLog>): string => {
  const flow = row.chainServices.map((svc) => formatService(svc)).join(" ➔ ");
  return row.chainExpanded
    ? $t("auditLogs.chain.collapseTooltip", { flow })
    : $t("auditLogs.chain.expandTooltip", {
        flow,
        count: row.mirrorChildren.length + 1,
      });
};

const relatedChainRecords = computed<AuditLog[]>(() => {
  const reqId = currentLog.value?.request_id?.trim();
  if (!reqId) return [];
  const matches = tableData.value.filter(
    (item) => item.request_id?.trim() === reqId,
  );
  const uniqueServices = new Set(matches.map((item) => item.service));
  return matches.length >= 2 && uniqueServices.size >= 2 ? matches : [];
});

const artPagination = computed<PaginationConfig>(() => ({
  current: pagination.page,
  size: pagination.limit,
  total: pagination.total,
}));

const exportItems = computed(() => [
  {
    command: "all",
    label: $t("common.actions.exportAll"),
    disabled: displayTableData.value.length === 0,
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
const ISO_DATETIME_RE = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/;

const formatDetailValue = (value: unknown, key?: string): string => {
  if (value === null || value === undefined || value === "") {
    return EMPTY_PLACEHOLDER;
  }
  if (typeof value === "boolean") {
    return value ? $t("auditLogs.values.yes") : $t("auditLogs.values.no");
  }
  if (typeof value === "number") {
    if (isAuditTimestampKey(key)) {
      return formatDate(value);
    }
    if (key === "content_length") {
      return `${value} B`;
    }
    if (isAuditDurationSecondsKey(key)) {
      return `${value}s`;
    }
    return String(value);
  }
  if (typeof value === "string") {
    if (
      (key === "category" || key === "mfa") &&
      MFA_CATEGORY_LABELS[value]
    ) {
      return $t(MFA_CATEGORY_LABELS[value]);
    }
    if (
      (key === "step_up_method" || key === "method") &&
      MFA_METHOD_LABELS[value]
    ) {
      return $t(MFA_METHOD_LABELS[value]);
    }
    if (isAuditTimestampKey(key)) {
      const asNum = Number(value);
      if (Number.isFinite(asNum) && value.trim() !== "") {
        return formatDate(asNum);
      }
      return formatDate(value);
    }
    if (ISO_DATETIME_RE.test(value)) {
      return formatDate(value);
    }
    return stripUrlScheme(value);
  }
  if (Array.isArray(value)) {
    if (!value.length) return $t("auditLogs.values.none");
    return value.map((item) => formatDetailValue(item, key)).join("、");
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
    .slice(0, 8)
    .map(([subKey, item]) => {
      const formatted =
        SENSITIVE_DETAIL_KEYS.has(subKey) && typeof item === "string"
          ? stripUrlScheme(item)
          : formatDetailValue(item, subKey);
      return `${formatDetailLabel(subKey)}：${formatted}`;
    });
  return summary.length ? summary.join("；") : EMPTY_PLACEHOLDER;
};

const formatRouteConfig = (config: Record<string, unknown>) => {
  const lines = [
    config.purpose_label
      ? `${$t("auditLogs.routeConfig.purpose")}${formatDetailValue(config.purpose_label, "purpose_label")}`
      : "",
    config.change_reason
      ? `${$t("auditLogs.routeConfig.changeReason")}${formatDetailValue(config.change_reason, "change_reason")}`
      : "",
    Array.isArray(config.tags) && config.tags.length
      ? `${$t("auditLogs.routeConfig.tags")}${formatDetailValue(config.tags, "tags")}`
      : "",
    config.effective_mode === "window"
      ? `${$t("auditLogs.routeConfig.effectiveWindow")}${formatDetailValue(config.effective_start_at, "effective_start_at")} ${$t("auditLogs.routeConfig.to")} ${formatDetailValue(config.effective_end_at, "effective_end_at")}`
      : $t("auditLogs.routeConfig.effectiveNow"),
    config.description
      ? `${$t("auditLogs.routeConfig.description")}${formatDetailValue(config.description, "description")}`
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
    ? condition.failure_types
        .map((item) => formatDetailValue(item))
        .join("、")
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
      ? `${$t("auditLogs.routeSnapshot.productId")}${formatDetailValue(routeRule.product_id, "product_id")}`
      : "",
    routeRule.tenant_id
      ? `${$t("auditLogs.routeSnapshot.tenantId")}${formatDetailValue(routeRule.tenant_id, "tenant_id")}`
      : $t("auditLogs.routeSnapshot.defaultRoute"),
    routeRule.priority
      ? `${$t("auditLogs.routeSnapshot.priority")}${formatDetailValue(routeRule.priority, "priority")}`
      : "",
    routeRule.status
      ? `${$t("auditLogs.routeSnapshot.status")}${formatDetailValue(routeRule.status, "status")}`
      : "",
    bindings.length
      ? `${$t("auditLogs.routeSnapshot.candidateCount")}${bindings.length}`
      : "",
    firstBinding.channel_id
      ? `${$t("auditLogs.routeSnapshot.firstChannel")}${formatDetailValue(firstBinding.channel_id, "channel_id")}`
      : "",
  ].filter(Boolean);
  return parts.join("；");
};

const formatActorName = (log: AuditLog) => {
  if (log.actor_name) return log.actor_name;
  return log.actor_type === "anonymous"
    ? $t("auditLogs.values.anonymous")
    : EMPTY_PLACEHOLDER;
};

const formatFailureReason = (reason?: string | null) => {
  if (!reason) return EMPTY_PLACEHOLDER;
  return FAILURE_REASON_LABELS[reason]
    ? $t(FAILURE_REASON_LABELS[reason])
    : reason.replaceAll("_", " ");
};

const changeRows = computed(() => {
  const changes = currentLog.value?.changes;
  const details = currentLog.value?.details || {};
  const beforeSource =
    changes && (changes.before !== undefined || changes.after !== undefined)
      ? changes.before
      : details.before;
  const afterSource =
    changes && (changes.before !== undefined || changes.after !== undefined)
      ? changes.after
      : details.after;
  if (beforeSource === undefined && afterSource === undefined) {
    return [];
  }
  return diffChanges(beforeSource, afterSource).map((row) => ({
    label: formatDetailLabel(row.key),
    before: formatDetailValue(row.before, row.key),
    after: formatDetailValue(row.after, row.key),
    changed: row.changed,
  }));
});

const auditDetailRows = computed(() => {
  const details = currentLog.value?.details || {};
  const hasExplicitChanges = Boolean(currentLog.value?.changes);
  return Object.entries(details)
    .filter(([key, value]) => {
      if (value === null || value === undefined || value === "") return false;
      if (key === "mfa" && details.category === value) return false;
      if (!hasExplicitChanges && (key === "before" || key === "after")) {
        return false;
      }
      return true;
    })
    .map(([key, value]) => ({
      key,
      label: formatDetailLabel(key),
      value: formatDetailValue(value, key),
    }));
});

const rawPayloadObject = computed(() => {
  if (!currentLog.value) return null;
  const payload: Record<string, unknown> = {};
  if (
    currentLog.value.details &&
    Object.keys(currentLog.value.details).length > 0
  ) {
    payload.details = currentLog.value.details;
  }
  if (currentLog.value.changes) {
    payload.changes = currentLog.value.changes;
  }
  if (currentLog.value.request_summary) {
    payload.request_summary = currentLog.value.request_summary;
  }
  return Object.keys(payload).length > 0 ? payload : null;
});

const hasRawJsonPayload = computed(() => Boolean(rawPayloadObject.value));
const rawJsonText = computed(() =>
  rawPayloadObject.value ? JSON.stringify(rawPayloadObject.value, null, 2) : "",
);

const handleCopyRawJson = async () => {
  if (!rawJsonText.value) return;
  await copyWithFeedback(rawJsonText.value);
};

const handleDrawerClosed = () => {
  rawJsonPanels.value = [];
};

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
  if (searchForm.includeSelfAudit) {
    params.include_self_audit = true;
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
    expandedRequestIds.value = new Set();
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
  searchForm.includeSelfAudit = false;
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

const handleSelectionChange = (selection: GroupedAuditRow<AuditLog>[]) => {
  selectedRows.value = selection;
};

const showDetails = async (row: AuditLog) => {
  detailVisible.value = true;
  detailLoading.value = true;
  try {
    currentLog.value = await fetchAuditLogById(row.id);
  } catch (error) {
    logger.error("Failed to load audit log detail:", error);
  } finally {
    detailLoading.value = false;
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
          actor_role: row.actor_role || EMPTY_PLACEHOLDER,
          action: formatAction(row.action),
          resource_type: formatResourceType(row.resource_type),
          status: $t(statusLabelKey(row.status)),
          failure_reason: formatFailureReason(row.failure_reason),
          ip_address: formatIpAddress(row.ip_address),
          request_id: row.request_id || EMPTY_PLACEHOLDER,
          route:
            [row.method, row.path].filter(Boolean).join(" ") ||
            EMPTY_PLACEHOLDER,
          status_code:
            row.status_code === null || row.status_code === undefined
              ? EMPTY_PLACEHOLDER
              : String(row.status_code),
          duration_ms:
            row.duration_ms === null || row.duration_ms === undefined
              ? EMPTY_PLACEHOLDER
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
.audit-self-toggle {
  height: 32px;
  margin-right: 0;
}

.audit-chain-badge {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  padding: 2px 8px;
  border: 1px solid var(--el-color-primary-light-5);
  border-radius: 4px;
  background-color: var(--el-color-primary-light-9);
  color: var(--el-color-primary);
  font-size: 12px;
  line-height: 18px;
  cursor: pointer;
  white-space: nowrap;
  transition: background-color 0.15s ease;
}

.audit-chain-badge:hover {
  background-color: var(--el-color-primary-light-8);
}

.audit-chain-badge__code {
  font-family: var(--el-font-family-mono, monospace);
}

.audit-chain-badge__icon {
  font-size: 14px;
}

.audit-mirror-cell {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  white-space: nowrap;
}

.audit-mirror-prefix {
  color: var(--el-text-color-secondary);
  font-size: 12px;
}

.audit-detail-body {
  min-height: 160px;
}

.audit-related-bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 8px 12px;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 6px;
  background-color: var(--el-fill-color-lighter);
}

.audit-related-bar__label {
  color: var(--el-text-color-secondary);
  font-size: 13px;
  font-weight: 500;
}

.audit-related-bar__items {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.audit-section-title {
  color: var(--el-text-color-primary);
  font-size: 14px;
  font-weight: 600;
}

.audit-main-descriptions :deep(.el-descriptions__table),
.audit-ext-descriptions :deep(.el-descriptions__table) {
  width: 100%;
  table-layout: fixed;
}

.audit-main-descriptions :deep(.el-descriptions__cell),
.audit-ext-descriptions :deep(.el-descriptions__cell) {
  padding: 6px 10px;
  line-height: 22px;
}

.audit-main-descriptions :deep(.el-descriptions__label),
.audit-ext-descriptions :deep(.el-descriptions__label) {
  white-space: nowrap;
}

.audit-main-descriptions :deep(.el-descriptions__content),
.audit-ext-descriptions :deep(.el-descriptions__content) {
  word-break: break-word;
}

.audit-nowrap-val {
  white-space: nowrap;
}

.audit-changed {
  font-weight: 600;
  color: var(--el-color-warning);
}

.audit-raw-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  padding-right: 12px;
  font-size: 13px;
  font-weight: 500;
}

.audit-raw-pre {
  margin: 0;
  padding: 12px;
  max-height: 280px;
  overflow: auto;
  border-radius: 6px;
  background-color: var(--el-fill-color-light);
  color: var(--el-text-color-regular);
  font-family: var(--el-font-family-mono, monospace);
  font-size: 12px;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-all;
}
</style>
