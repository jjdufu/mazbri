<template>
  <ControlPanelPageShell
    :title="$t('menus.mazbri.usageReconciliation')"
    icon="ri:file-search-line"
  >
    <div
      ref="pageRef"
      class="usage-recon-page"
      style="display: flex; flex-direction: column; flex: 1; min-height: 0"
    >
      <div
        v-roles="['R_ADMIN', 'R_FINANCE']"
        v-loading="settingsLoading"
        class="usage-recon-settings"
      >
        <div class="usage-recon-settings__header">
          <ElButton
            link
            type="primary"
            class="usage-recon-settings__toggle-btn"
            @click="settingsExpanded = !settingsExpanded"
          >
            <ArtSvgIcon
              :icon="
                settingsExpanded ? 'ri:arrow-up-s-line' : 'ri:arrow-down-s-line'
              "
              class="usage-recon-settings__toggle-icon"
            />
            <ArtSvgIcon
              icon="ri:settings-3-line"
              class="usage-recon-settings__toggle-gear"
            />
            {{ $t("billing.reconciliation.settingsTitle") }}
            <span class="usage-recon-settings__toggle-meta">
              {{ settingsSummaryLine }}
            </span>
          </ElButton>
          <ElButton
            v-if="settingsExpanded"
            type="primary"
            :loading="settingsSaving"
            @click="handleSaveSettings"
          >
            {{ $t("common.save") }}
          </ElButton>
        </div>

        <Transition name="usage-recon-fade">
          <ElCard
            v-if="settingsExpanded"
            shadow="never"
            class="usage-recon-settings__panel"
          >
            <div class="usage-recon-settings__body">




              <div class="usage-recon-settings__grid">
                <CommonSettingRow
                  mode="inline"
                  :label="$t('billing.reconciliation.scheduledEnabled')"
                >
                  <ElSwitch v-model="settingsForm.schedule.enabled" />
                </CommonSettingRow>
                <CommonSettingRow
                  mode="inline"
                  :label="$t('billing.reconciliation.intervalSecs')"
                >
                  <ElInputNumber
                    v-model="settingsForm.schedule.interval_secs"
                    :min="60"
                    :step="60"
                    controls-position="right"
                    class="setting-control"
                  />
                </CommonSettingRow>

                <CommonSettingRow
                  mode="inline"
                  :label="$t('billing.reconciliation.usePreviousDay')"
                >
                  <ElSwitch
                    v-model="settingsForm.schedule.use_previous_day"
                  />
                </CommonSettingRow>
                <CommonSettingRow
                  mode="inline"
                  :label="$t('billing.reconciliation.lossRatioThreshold')"
                >
                  <ElInputNumber
                    v-model="settingsForm.loss_ratio_threshold"
                    :min="0"
                    :max="1"
                    :step="0.01"
                    :precision="4"
                    controls-position="right"
                    class="setting-control"
                  />
                </CommonSettingRow>

                <CommonSettingRow
                  mode="inline"
                  :label="$t('billing.reconciliation.alertOnFlagged')"
                >
                  <ElSwitch v-model="settingsForm.alert.on_flagged" />
                </CommonSettingRow>

                <CommonSettingRow
                  mode="inline"
                  :label="$t('billing.reconciliation.warnBeforeBill')"
                >
                  <ElSwitch
                    v-model="
                      settingsForm.warn_on_open_discrepancies_before_bill
                    "
                  />
                </CommonSettingRow>
                <CommonSettingRow
                  mode="inline"
                  :label="$t('billing.reconciliation.alertWebhook')"
                >
                  <ExpandOnFocusInput
                    v-model="settingsForm.alert.webhook_url"
                    clearable
                    :compact-width="180"
                    :expand-width="420"
                    class="setting-control"
                    :placeholder="
                      $t('billing.reconciliation.alertWebhookPlaceholder')
                    "
                    :disabled="!settingsForm.alert.on_flagged"
                  />
                </CommonSettingRow>
              </div>
            </div>
          </ElCard>
        </Transition>
      </div>

      <ResourceTablePageShell
        v-model:columns="columns"
        v-model="searchKeyword"
        v-model:filters="listFilters"
        :filter-catalog="reconciliationCatalog"
        :title="$t('billing.reconciliation.listTitle')"
        :loading="loading"
        :data="items"
        :empty-text="emptyDescription"
        :pagination="artPagination"
        :show-import="false"
        :show-export="true"
        :export-items="exportItems"
        :export-disabled="items.length === 0 || exporting"
        :export-loading="exporting"
        :export-label="$t('billing.actions.export')"
        :show-primary="true"
        :primary-label="$t('billing.reconciliation.runAudit')"
        primary-icon="ri:play-circle-line"
        :primary-roles="['R_ADMIN', 'R_FINANCE']"
        :show-delete="true"
        :delete-total-count="items.length"
        :delete-selected-count="selectedRows.length"
        :delete-roles="['R_ADMIN', 'R_FINANCE']"
        :fit="false"
        table-layout="auto"
        table-class="surface-panel-table usage-recon-table"
        @refresh="loadData"
        @selection-change="handleSelectionChange"
        @pagination:size-change="handlePageSizeChange"
        @pagination:current-change="handleCurrentPageChange"
        @search="handleSearch"
        @export="
          (command: 'page' | 'selected') => handleExportCommand(command)
        "
        @primary="openRunDialog"
        @delete="
          (type: 'all' | 'selected') => handleDeleteCommand(type, items)
        "
      >
        <template #after-search>
          <TableHeaderDatePickerPopover
            v-model="filterRange"
            type="daterange"
            value-format="YYYY-MM-DD"
            :start-placeholder="$t('billing.reconciliation.from')"
            :end-placeholder="$t('billing.reconciliation.to')"
            @change="handleFilterChange"
          />
        </template>

        <template #reconciliation_date="{ row }">
          {{ row.reconciliation_date }}
        </template>
        <template #billable_call_count="{ row }">
          {{ formatCount(row.billable_call_count) }}
        </template>
        <template #gateway_call_count="{ row }">
          {{ formatCount(row.gateway_call_count) }}
        </template>
        <template #upstream_preagg_call_count="{ row }">
          {{ formatCount(row.upstream_preagg_call_count) }}
        </template>
        <template #discrepancy_ratio="{ row }">
          {{ formatRatio(row.discrepancy_ratio) }}
        </template>
        <template #status="{ row }">
          <ElTag :type="statusTagType(row.status)" size="small">
            {{ statusLabel(row.status) }}
          </ElTag>
        </template>
        <template #created_at="{ row }">
          {{ formatDate(row.created_at) }}
        </template>
        <template #operations="{ row }">
          <TableActionRow>
            <ElButton link type="primary" @click="openDetail(row)">
              {{ $t("common.actions.details") }}
            </ElButton>
            <ElButton
              link
              type="primary"
              @click="openCallMonitor(row, 'customer')"
            >
              {{ $t("billing.reconciliation.viewCustomerCalls") }}
            </ElButton>
            <ElButton
              link
              type="primary"
              @click="openCallMonitor(row, 'upstream')"
            >
              {{ $t("billing.reconciliation.viewUpstreamCalls") }}
            </ElButton>
            <ElButton
              v-if="canAcknowledge(row.status)"
              v-roles="['R_ADMIN', 'R_FINANCE']"
              link
              type="warning"
              @click="openResolve(row, 'acknowledged')"
            >
              {{ $t("billing.reconciliation.acknowledge") }}
            </ElButton>
            <ElButton
              v-if="canResolve(row.status)"
              v-roles="['R_ADMIN', 'R_FINANCE']"
              link
              type="success"
              @click="openResolve(row, 'resolved')"
            >
              {{ $t("billing.reconciliation.resolve") }}
            </ElButton>
            <ElButton
              v-roles="['R_ADMIN', 'R_FINANCE']"
              link
              type="danger"
              @click="handleDeleteRow(row)"
            >
              {{ $t("common.actions.delete") }}
            </ElButton>
          </TableActionRow>
        </template>
      </ResourceTablePageShell>
    </div>

    <AdaptiveDrawer v-model="detailVisible" size="680px" destroy-on-close>
      <template #header>
        <CommonDrawerHeader :title="$t('billing.reconciliation.detailTitle')" />
      </template>
      <div v-if="currentRow" class="usage-recon-detail">

        <ElDescriptions :column="1" border size="small">
          <ElDescriptionsItem :label="$t('billing.reconciliation.columns.date')">
            {{ currentRow.reconciliation_date }}
          </ElDescriptionsItem>
          <ElDescriptionsItem :label="$t('billing.reconciliation.columns.scope')">
            {{ currentRow.scope_type
            }}{{ currentRow.scope_id ? ` / ${currentRow.scope_id}` : "" }}
          </ElDescriptionsItem>
          <ElDescriptionsItem :label="$t('billing.reconciliation.columns.status')">
            <ElTag :type="statusTagType(currentRow.status)" size="small">
              {{ statusLabel(currentRow.status) }}
            </ElTag>
          </ElDescriptionsItem>
          <ElDescriptionsItem :label="$t('billing.reconciliation.note')">
            {{ currentRow.resolution_note || "-" }}
          </ElDescriptionsItem>
          <ElDescriptionsItem :label="$t('common.fields.createdAt')">
            {{ formatDate(currentRow.created_at) }}
          </ElDescriptionsItem>
          <ElDescriptionsItem
            v-if="currentRow.resolved_at"
            :label="$t('billing.reconciliation.resolvedAt')"
          >
            {{ formatDate(currentRow.resolved_at) }}
          </ElDescriptionsItem>
        </ElDescriptions>


        <ElAlert
          v-if="detailView?.errorText"
          type="error"
          :closable="false"
          show-icon
          :title="$t('billing.reconciliation.detail.errorTitle')"
          :description="detailView.errorText"
        />
        <section
          v-if="detailView"
          class="usage-recon-detail__section usage-recon-detail__summary"
        >
          <h4 class="usage-recon-detail__section-title">
            {{ $t("billing.reconciliation.detail.summaryTitle") }}
          </h4>
          <div class="usage-recon-detail__summary-grid">
            <div class="usage-recon-detail__summary-item">
              <span class="usage-recon-detail__summary-label">
                {{ $t("billing.reconciliation.columns.ratio") }}
              </span>
              <span class="usage-recon-detail__summary-value">
                {{ formatRatio(currentRow.discrepancy_ratio) }}
                <template v-if="detailView.threshold != null">
                  <span class="usage-recon-detail__muted">
                    ({{
                      $t("billing.reconciliation.detail.threshold", {
                        value: formatRatio(detailView.threshold),
                      })
                    }})
                  </span>
                </template>
              </span>
            </div>
            <div class="usage-recon-detail__summary-item">
              <span class="usage-recon-detail__summary-label">
                {{ $t("billing.reconciliation.detail.vsThreshold") }}
              </span>
              <ElTag
                :type="detailView.overThreshold ? 'danger' : 'success'"
                size="small"
              >
                {{
                  detailView.overThreshold
                    ? $t("billing.reconciliation.detail.overThreshold")
                    : $t("billing.reconciliation.detail.withinThreshold")
                }}
              </ElTag>
            </div>
            <div class="usage-recon-detail__summary-item usage-recon-detail__summary-item--full">
              <span class="usage-recon-detail__summary-label">
                {{ $t("billing.reconciliation.detail.primaryCompare") }}
              </span>
              <span class="usage-recon-detail__summary-value">
                {{
                  $t("billing.reconciliation.detail.primaryCompareLine", {
                    billing: formatCount(detailView.oltpCalls),
                    peer: formatCount(detailView.comparisonCalls),
                    peerLabel: detailView.comparisonLabel,
                  })
                }}
              </span>
            </div>
            <div class="usage-recon-detail__summary-item usage-recon-detail__summary-item--full">
              <span class="usage-recon-detail__summary-label">
                {{ $t("billing.reconciliation.detail.callDiff") }}
              </span>
              <span
                class="usage-recon-detail__summary-value"
                :class="{
                  'usage-recon-detail__summary-value--warn':
                    detailView.callDiffAbs > 0,
                }"
              >
                {{ detailView.callDiffText }}
              </span>
            </div>
          </div>
        </section>


        <section v-if="detailView" class="usage-recon-detail__section">
          <h4 class="usage-recon-detail__section-title">
            {{ $t("billing.reconciliation.detail.callsTitle") }}
          </h4>
          <ElTable
            :data="detailView.callRows"
            size="small"
            border
            class="usage-recon-detail__calls-table"
          >
            <ElTableColumn
              prop="label"
              :label="$t('billing.reconciliation.detail.sourceCol')"
              min-width="160"
            />
            <ElTableColumn
              prop="countText"
              :label="$t('billing.reconciliation.detail.countCol')"
              min-width="110"
              align="right"
            />
            <ElTableColumn
              prop="role"
              :label="$t('billing.reconciliation.detail.roleCol')"
              min-width="120"
            >
              <template #default="{ row }">
                <ElTag
                  v-if="row.primary"
                  type="primary"
                  size="small"
                  effect="plain"
                >
                  {{ row.role }}
                </ElTag>
                <span v-else class="usage-recon-detail__muted">{{
                  row.role
                }}</span>
              </template>
            </ElTableColumn>
          </ElTable>
        </section>


        <section
          v-if="detailView?.amount"
          class="usage-recon-detail__section"
        >
          <h4 class="usage-recon-detail__section-title">
            {{ $t("billing.reconciliation.detail.amountTitle") }}
          </h4>
          <ElDescriptions :column="1" border size="small">
            <ElDescriptionsItem
              :label="$t('billing.reconciliation.detail.usageAmount')"
            >
              {{ formatAmount(detailView.amount.usageBillable) }}
            </ElDescriptionsItem>
            <ElDescriptionsItem
              :label="$t('billing.reconciliation.detail.billItemsTotal')"
            >
              {{ formatAmount(detailView.amount.billItemsTotal) }}
            </ElDescriptionsItem>
            <ElDescriptionsItem
              :label="$t('billing.reconciliation.detail.amountDiff')"
            >
              <span
                :class="{
                  'usage-recon-detail__amount-ok':
                    detailView.amount.diffIsZero,
                  'usage-recon-detail__summary-value--warn':
                    !detailView.amount.diffIsZero,
                }"
              >
                {{ formatAmount(detailView.amount.diff) }}
                <template v-if="detailView.amount.diffIsZero">
                  · {{ $t("billing.reconciliation.detail.amountMatch") }}
                </template>
              </span>
            </ElDescriptionsItem>
          </ElDescriptions>
        </section>


        <ElCollapse
          v-if="detailView"
          v-model="detailCollapseNames"
          class="usage-recon-detail__collapse"
        >
          <ElCollapseItem
            name="methodology"
            :title="$t('billing.reconciliation.detail.methodologyTitle')"
          >
            <ul class="usage-recon-detail__methodology">
              <li
                v-for="(line, idx) in detailView.methodologyLines"
                :key="idx"
              >
                {{ line }}
              </li>
            </ul>
          </ElCollapseItem>
          <ElCollapseItem
            v-if="currentRow.details"
            name="raw"
            :title="$t('billing.reconciliation.detail.rawSnapshotTitle')"
          >
            <pre class="usage-recon-detail__json">{{
              JSON.stringify(currentRow.details, null, 2)
            }}</pre>
          </ElCollapseItem>
        </ElCollapse>
      </div>
    </AdaptiveDrawer>

    <EditDialog
      v-model="resolveVisible"
      :title="
        resolveTarget === 'resolved'
          ? $t('billing.reconciliation.resolveTitle')
          : $t('billing.reconciliation.acknowledgeTitle')
      "
      width="480px"
      :confirm-text="$t('common.confirm')"
      :saving="resolveLoading"
      @confirm="handleSubmitResolve"
    >
      <ElForm label-position="top" @submit.prevent>
        <ElFormItem :label="$t('billing.reconciliation.note')">
          <ElInput
            v-model="resolveNote"
            type="textarea"
            :rows="3"
            maxlength="500"
            show-word-limit
            :placeholder="$t('billing.reconciliation.notePlaceholder')"
          />
        </ElFormItem>
      </ElForm>
    </EditDialog>

    <EditDialog
      v-model="runDialogVisible"
      :title="$t('billing.reconciliation.runAudit')"
      width="420px"
      :confirm-text="$t('billing.reconciliation.runAudit')"
      :saving="runLoading"
      @confirm="handleRunAudit"
    >
      <ElForm label-position="top" @submit.prevent>
        <ElFormItem :label="$t('billing.reconciliation.runDatePlaceholder')">
          <DatePicker
            v-model="runDate"
            type="date"
            value-format="YYYY-MM-DD"
            :placeholder="$t('billing.reconciliation.runDatePlaceholder')"
            style="width: 100%"
          />
        </ElFormItem>
      </ElForm>
    </EditDialog>
  </ControlPanelPageShell>
</template>

<script setup lang="ts">
import { ref } from "vue";
import type { ListFilter } from "@/types/list-query";
import { reconciliationCatalog } from "@/list-catalog/common-resources";
import { ElMessage, ElMessageBox } from "element-plus";
import { useRouter } from "vue-router";
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import ResourceTablePageShell from "@/components/common/resource-table-page-shell/index.vue";
import CommonDrawerHeader from "@/components/common/drawer-header/index.vue";
import CommonSettingRow from "@/components/common/setting-row/index.vue";
import ExpandOnFocusInput from "@/components/common/expand-on-focus-input/index.vue";
import TableHeaderDatePickerPopover from "@/components/common/table-header-date-picker-popover/index.vue";
import DatePicker from "@/components/common/DatePicker.vue";
import AdaptiveDrawer from "@/components/common/AdaptiveDrawer.vue";
import EditDialog from "@/components/common/EditDialog.vue";
import {
  deleteReconciliationDiscrepancy,
  fetchReconciliationDiscrepancies,
  fetchReconciliationSettings,
  runReconciliationAudit,
  updateReconciliationDiscrepancy,
  updateReconciliationSettings,
  type ReconciliationDiscrepancy,
  type ReconciliationSettings,
} from "@/api/billing";
import { $t } from "@/locales";
import { runWithLoading } from "@/composables/useAsyncState";
import { useTableSelection } from "@/composables/useTableSelection";
import { logger } from "@/utils/sys";
import { formatAmount, formatDate } from "@/utils/format";
import { downloadCsv, serializeRawCsv } from "@/utils/csv";
import type { ColumnOption, PaginationConfig } from "@/types/component";

defineOptions({ name: "UsageReconciliation" });

const router = useRouter();
const pageRef = ref<HTMLElement | null>(null);
const loading = ref(false);
const runLoading = ref(false);
const exporting = ref(false);
const settingsLoading = ref(false);
const settingsSaving = ref(false);

const settingsExpanded = ref(false);
const settingsForm = reactive<ReconciliationSettings>({
  schedule: { enabled: false, interval_secs: 86400, use_previous_day: true },
  alert: { on_flagged: false, webhook_url: "" },
  sources: { include_amount_recon: true },
  loss_ratio_threshold: 0.05,
  default_scope_type: "system",
  default_scope_id: "",
  warn_on_open_discrepancies_before_bill: true,
  last_scheduled_run_at: null,
});
const items = ref<ReconciliationDiscrepancy[]>([]);

const filterRange = ref<[string, string] | null>(null);

const searchForm = reactive({
  q: "" as string,
});
const listFilters = ref<ListFilter[]>([]);
const searchKeyword = computed<string>({
  get: () => searchForm.q ?? "",
  set: (value) => {
    searchForm.q = value;
  },
});
const hasActiveSearch = computed(
  () => Boolean(searchForm.q?.trim()) || listFilters.value.length > 0,
);
const emptyDescription = computed(() =>
  hasActiveSearch.value || Array.isArray(filterRange.value)
    ? $t("billing.reconciliation.emptyNoMatch")
    : $t("billing.reconciliation.empty"),
);
const runDate = ref("");
const runDialogVisible = ref(false);
const pagination = reactive({
  page: 1,
  page_size: 20,
  total: 0,
});
const exportItems = computed(() => [
  {
    command: "page",
    label: $t("billing.actions.exportPage"),
    disabled: items.value.length === 0,
  },
  {
    command: "selected",
    label: $t("billing.actions.exportSelected", {
      count: selectedRows.value.length,
    }),
    disabled: selectedRows.value.length === 0,
  },
]);
const { selectedRows, handleSelectionChange, handleDeleteCommand } =
  useTableSelection<ReconciliationDiscrepancy>({
    deleteApi: (row) => deleteReconciliationDiscrepancy(row.id),
    getItemName: (row) =>
      `${row.reconciliation_date} · ${statusLabel(row.status)}`,
    getItemId: (row) => row.id,
    onDeleteSuccess: () => {
      void loadData();
    },
    allDeleteConfirmMsg: (count) =>
      $t("billing.reconciliation.deleteConfirm.all", { count }),
    selectedDeleteConfirmMsg: (count) =>
      $t("billing.reconciliation.deleteConfirm.selected", { count }),
    actionLabel: $t("common.actions.delete"),
  });
const columns = ref<ColumnOption<ReconciliationDiscrepancy>[]>([
  { type: "selection", width: 55 },
  {
    prop: "reconciliation_date",
    label: $t("billing.reconciliation.columns.date"),
    minWidth: 96,
    useSlot: true,
  },
  {
    prop: "billable_call_count",
    label: $t("billing.reconciliation.columns.oltp"),
    minWidth: 110,
    useSlot: true,
  },
  {
    prop: "gateway_call_count",
    label: $t("billing.reconciliation.columns.gateway"),
    minWidth: 110,
    useSlot: true,
  },
  {
    prop: "upstream_preagg_call_count",
    label: $t("billing.reconciliation.columns.olap"),
    minWidth: 110,
    useSlot: true,
  },
  {
    prop: "discrepancy_ratio",
    label: $t("billing.reconciliation.columns.ratio"),
    minWidth: 110,
    useSlot: true,
  },
  {
    prop: "status",
    label: $t("billing.reconciliation.columns.status"),
    minWidth: 110,
    useSlot: true,
  },
  {
    prop: "created_at",
    label: $t("common.fields.createdAt"),
    minWidth: 112,
    useSlot: true,
  },
  {
    prop: "operations",
    label: $t("common.fields.operations"),
    width: 400,
    fixed: "right",
    useSlot: true,
  },
]);
const artPagination = computed<PaginationConfig>(() => ({
  current: pagination.page,
  size: pagination.page_size,
  total: pagination.total,
}));

const detailVisible = ref(false);
const currentRow = ref<ReconciliationDiscrepancy | null>(null);

const detailCollapseNames = ref<string[]>([]);
const resolveVisible = ref(false);
const resolveLoading = ref(false);
const resolveNote = ref("");
const resolveTarget = ref<"acknowledged" | "resolved">("resolved");
const resolveRow = ref<ReconciliationDiscrepancy | null>(null);

const formatRatio = (value: string | number | null | undefined) => {
  const n = Number(value ?? 0);
  if (Number.isNaN(n)) return String(value ?? "-");
  return `${(n * 100).toFixed(2)}%`;
};

const formatCount = (value: string | number | null | undefined) => {
  const n = Number(value ?? 0);
  if (!Number.isFinite(n)) return String(value ?? "-");
  return new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 0 }).format(n);
};

const asRecord = (value: unknown): Record<string, unknown> | null => {
  if (value && typeof value === "object" && !Array.isArray(value)) {
    return value as Record<string, unknown>;
  }
  return null;
};

const asNumber = (value: unknown, fallback = 0): number => {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string" && value.trim() !== "") {
    const n = Number(value);
    if (Number.isFinite(n)) return n;
  }
  return fallback;
};

const asString = (value: unknown): string | null => {
  if (typeof value === "string" && value.trim() !== "") return value;
  if (typeof value === "number" && Number.isFinite(value)) return String(value);
  return null;
};


const sourceLabel = (code: string | null | undefined) => {
  if (!code) return $t("billing.reconciliation.detail.sources.unknown");
  const key = `billing.reconciliation.detail.sources.${code}`;
  const label = $t(key);
  return label === key ? code : label;
};

type DetailCallRow = {
  label: string;
  count: number;
  countText: string;
  role: string;
  primary: boolean;
};

type DetailAmountView = {
  usageBillable: string;
  billItemsTotal: string;
  diff: string;
  diffIsZero: boolean;
};

type DetailView = {
  oltpCalls: number;
  comparisonCalls: number;
  comparisonLabel: string;
  threshold: number | null;
  overThreshold: boolean;
  callDiffAbs: number;
  callDiffText: string;
  callRows: DetailCallRow[];
  amount: DetailAmountView | null;
  methodologyLines: string[];
  errorText: string | null;
};

const detailView = computed<DetailView | null>(() => {
  const row = currentRow.value;
  if (!row) return null;

  const details = asRecord(row.details) ?? {};
  const oltpCalls = row.billable_call_count;
  const gatewayCalls = row.gateway_call_count;
  const upstreamCalls = row.upstream_preagg_call_count;
  const comparisonLabel = sourceLabel("request_logs_gateway");
  const errorText =
    row.status === "error" ? (asString(details.error) ?? "-") : null;
  const thresholdRaw = asString(details.loss_ratio_threshold);
  const threshold =
    thresholdRaw != null && Number.isFinite(Number(thresholdRaw))
      ? Number(thresholdRaw)
      : null;
  const ratio = Number(row.discrepancy_ratio ?? 0);
  const overThreshold =
    threshold != null && Number.isFinite(ratio)
      ? ratio > threshold
      : row.status === "flagged";

  const callDiffAbs = Math.abs(oltpCalls - gatewayCalls);
  let callDiffText: string;
  if (callDiffAbs === 0) {
    callDiffText = $t("billing.reconciliation.detail.callDiffMatch");
  } else if (oltpCalls < gatewayCalls) {
    callDiffText = $t("billing.reconciliation.detail.callDiffBillingLess", {
      count: formatCount(callDiffAbs),
      peerLabel: comparisonLabel,
    });
  } else {
    callDiffText = $t("billing.reconciliation.detail.callDiffBillingMore", {
      count: formatCount(callDiffAbs),
      peerLabel: comparisonLabel,
    });
  }

  const callRows: DetailCallRow[] = [
    {
      label: $t("billing.reconciliation.detail.sourceBilling"),
      count: oltpCalls,
      countText: formatCount(oltpCalls),
      role: $t("billing.reconciliation.detail.roleBilling"),
      primary: false,
    },
    {
      label: comparisonLabel,
      count: gatewayCalls,
      countText: formatCount(gatewayCalls),
      role: $t("billing.reconciliation.detail.rolePrimary"),
      primary: true,
    },
    {
      label: $t("billing.reconciliation.detail.sourceUpstream"),
      count: upstreamCalls,
      countText: formatCount(upstreamCalls),
      role: $t("billing.reconciliation.detail.roleReference"),
      primary: false,
    },
  ];

  let amount: DetailAmountView | null = null;
  const amountObj = asRecord(details.amount_recon);
  if (amountObj) {
    const usageBillable =
      asString(amountObj.usage_billable_amount) ?? "0";
    const billItemsTotal = asString(amountObj.bill_items_total) ?? "0";
    const diff = asString(amountObj.amount_diff) ?? "0";
    const diffN = Math.abs(Number(diff));
    amount = {
      usageBillable,
      billItemsTotal,
      diff,
      diffIsZero: !Number.isFinite(diffN) || diffN < 1e-9,
    };
  }

  const methodologyLines: string[] = [
    $t("billing.reconciliation.detail.methodBilling"),
    $t("billing.reconciliation.detail.methodGateway"),
    $t("billing.reconciliation.detail.methodUpstream"),
    $t("billing.reconciliation.detail.methodPrimary", {
      peerLabel: comparisonLabel,
    }),
  ];

  return {
    oltpCalls,
    comparisonCalls: gatewayCalls,
    comparisonLabel,
    threshold,
    overThreshold,
    callDiffAbs,
    callDiffText,
    callRows,
    amount,
    methodologyLines,
    errorText,
  };
});


const settingsSummaryLine = computed(() => {
  const scheduled = settingsForm.schedule.enabled
    ? $t("billing.reconciliation.summaryScheduledOn")
    : $t("billing.reconciliation.summaryScheduledOff");
  const threshold = $t("billing.reconciliation.summaryThreshold", {
    value: formatRatio(settingsForm.loss_ratio_threshold),
  });
  const last = settingsForm.last_scheduled_run_at
    ? formatDate(settingsForm.last_scheduled_run_at)
    : $t("billing.reconciliation.neverRun");
  return `${scheduled} · ${threshold} · ${last}`;
});

const statusLabel = (status: string) => {
  const key = `billing.reconciliation.statuses.${status}`;
  const label = $t(key);
  return label === key ? status : label;
};

const statusTagType = (
  status: string,
): "success" | "warning" | "info" | "danger" | "primary" => {
  switch (status) {
    case "ok":
      return "success";
    case "flagged":
    case "error":
      return "danger";
    case "acknowledged":
      return "warning";
    case "resolved":
      return "info";
    default:
      return "info";
  }
};

const canAcknowledge = (status: string) =>
  status === "ok" || status === "flagged" || status === "error";
const canResolve = (status: string) =>
  status === "ok" ||
  status === "flagged" ||
  status === "error" ||
  status === "acknowledged";

const loadData = async () => {
  await runWithLoading(loading, async () => {
    const range = Array.isArray(filterRange.value) ? filterRange.value : null;
    const res = await fetchReconciliationDiscrepancies({
      filters: listFilters.value.length
        ? JSON.stringify(listFilters.value)
        : undefined,
      from: range?.[0],
      to: range?.[1],
      q: searchForm.q?.trim() || undefined,
      page: pagination.page,
      page_size: pagination.page_size,
    });
    items.value = res.items ?? [];
    pagination.total = res.total ?? 0;
  }).catch((error) => {
    logger.error("Failed to load reconciliation discrepancies:", error);
  });
};

const handleSearch = () => {
  pagination.page = 1;
  void loadData();
};


const handleFilterChange = () => {
  pagination.page = 1;
  void loadData();
};

const handlePageSizeChange = (size: number) => {
  pagination.page_size = size;
  pagination.page = 1;
  void loadData();
};

const handleCurrentPageChange = (page: number) => {
  pagination.page = page;
  void loadData();
};

const openRunDialog = () => {
  if (!runDate.value) {
    runDate.value = new Date().toISOString().slice(0, 10);
  }
  runDialogVisible.value = true;
};

const handleRunAudit = async () => {
  if (!runDate.value) {
    ElMessage.warning($t("billing.reconciliation.runDateRequired"));
    return;
  }
  await runWithLoading(runLoading, async () => {
    const result = await runReconciliationAudit({
      reconciliation_date: runDate.value,
      scope_type: "system",
    });
    ElMessage.success(
      $t("billing.reconciliation.runSuccess", {
        status: statusLabel(result.status),
      }),
    );
    runDialogVisible.value = false;
    await loadData();
  }).catch((error) => {
    logger.error("Failed to run reconciliation audit:", error);
  });
};

const exportRowsToCsv = (rows: ReconciliationDiscrepancy[], suffix: string) => {
  if (rows.length === 0) {
    ElMessage.warning($t("billing.reconciliation.exportEmpty"));
    return;
  }
  const header = [
    $t("billing.reconciliation.columns.date"),
    $t("billing.reconciliation.columns.oltp"),
    $t("billing.reconciliation.columns.gateway"),
    $t("billing.reconciliation.columns.olap"),
    $t("billing.reconciliation.columns.ratio"),
    $t("billing.reconciliation.columns.status"),
    $t("billing.reconciliation.note"),
    $t("common.fields.createdAt"),
  ];
  const body = rows.map((r) => [
    r.reconciliation_date,
    r.billable_call_count,
    r.gateway_call_count,
    r.upstream_preagg_call_count,
    formatRatio(r.discrepancy_ratio),
    statusLabel(r.status),
    r.resolution_note || "",
    r.created_at,
  ]);
  downloadCsv(
    `\uFEFF${serializeRawCsv([header, ...body])}\n`,
    `usage-reconciliation-${suffix}-${Date.now()}.csv`,
  );
  ElMessage.success($t("billing.reconciliation.exportSuccess"));
};

const handleExportCommand = (command: "page" | "selected") => {
  exporting.value = true;
  try {
    if (command === "selected") {
      exportRowsToCsv([...selectedRows.value], "selected");
      return;
    }
    exportRowsToCsv([...items.value], "page");
  } finally {
    exporting.value = false;
  }
};

const handleDeleteRow = async (row: ReconciliationDiscrepancy) => {
  try {
    await ElMessageBox.confirm(
      $t("billing.reconciliation.deleteConfirm.one", {
        name: `${row.reconciliation_date} · ${statusLabel(row.status)}`,
      }),
      $t("billing.reconciliation.deleteConfirm.title"),
      {
        confirmButtonText: $t("common.actions.delete"),
        cancelButtonText: $t("common.cancel"),
        type: "warning",
      },
    );
    await deleteReconciliationDiscrepancy(row.id);
    selectedRows.value = [];
    await loadData();
  } catch (error) {
    if (error !== "cancel") {
      logger.error("Failed to delete reconciliation discrepancy:", error);
    }
  }
};

const openDetail = (row: ReconciliationDiscrepancy) => {
  currentRow.value = row;
  detailCollapseNames.value = [];
  detailVisible.value = true;
};

const openResolve = (
  row: ReconciliationDiscrepancy,
  target: "acknowledged" | "resolved",
) => {
  resolveRow.value = row;
  resolveTarget.value = target;
  resolveNote.value = row.resolution_note ?? "";
  resolveVisible.value = true;
};

const handleSubmitResolve = async () => {
  if (!resolveRow.value) return;
  resolveLoading.value = true;
  try {
    await updateReconciliationDiscrepancy(resolveRow.value.id, {
      status: resolveTarget.value,
      resolution_note: resolveNote.value.trim() || undefined,
    });
    resolveVisible.value = false;
    await loadData();
  } catch (error) {
    logger.error("Failed to update reconciliation discrepancy:", error);
  } finally {
    resolveLoading.value = false;
  }
};


const openCallMonitor = (
  row: ReconciliationDiscrepancy,
  mode: "customer" | "upstream",
) => {
  const day = row.reconciliation_date;
  if (!day) return;
  const start = new Date(`${day}T00:00:00`).getTime();
  const end = new Date(`${day}T23:59:59.999`).getTime();
  const path =
    mode === "upstream"
      ? "/monitoring/upstream-calls"
      : "/monitoring/customer-calls";
  void router.push({
    path,
    query: {
      start_time: String(Math.floor(start / 1000)),
      end_time: String(Math.floor(end / 1000)),
    },
  });
};

const loadSettings = async () => {
  settingsLoading.value = true;
  try {
    const res = await fetchReconciliationSettings();
    Object.assign(settingsForm, res);
  } catch (error) {
    logger.error("Failed to load reconciliation settings:", error);
  } finally {
    settingsLoading.value = false;
  }
};

const handleSaveSettings = async () => {
  settingsSaving.value = true;
  try {
    const saved = await updateReconciliationSettings({ ...settingsForm });
    Object.assign(settingsForm, saved);
    ElMessage.success($t("billing.reconciliation.settingsSaved"));
  } catch (error) {
    logger.error("Failed to save reconciliation settings:", error);
  } finally {
    settingsSaving.value = false;
  }
};

onMounted(() => {
  if (!runDate.value) {
    const d = new Date();
    runDate.value = d.toISOString().slice(0, 10);
  }
  void loadSettings();
  void loadData();
});
</script>

<style scoped lang="scss">
.usage-recon-page {
  min-width: 0;
}

.usage-recon-settings {
  flex-shrink: 0;
}

.usage-recon-settings__header {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 2px 2px 0;
}

.usage-recon-settings__toggle-btn {
  padding: 0;
  font-size: 13px;
  font-weight: 600;
  max-width: 100%;
  height: auto;
  white-space: normal;
  text-align: left;
}

.usage-recon-settings__toggle-icon {
  margin-right: 2px;
  font-size: 14px;
  flex-shrink: 0;
}

.usage-recon-settings__toggle-gear {
  margin-right: 4px;
  font-size: 14px;
  flex-shrink: 0;
}

.usage-recon-settings__toggle-meta {
  margin-left: 8px;
  color: var(--el-text-color-secondary);
  font-weight: 500;
  font-size: 12px;
}

.usage-recon-settings__panel {
  margin-top: 10px;
  border: 1px solid var(--art-card-border, var(--el-border-color-lighter));
}

.usage-recon-settings__body {
  display: flex;
  flex-direction: column;
  gap: 14px;
}


.usage-recon-settings__grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  column-gap: 48px;
  row-gap: 14px;
  align-items: center;
}


.setting-control {
  width: 180px !important;
  max-width: 100%;
  box-sizing: border-box;
}

:deep(.common-setting-row) {
  min-width: 0;
  min-height: 36px;
  width: 100%;
  box-sizing: border-box;
}

:deep(.common-setting-row--inline) {
  display: flex;
  align-items: center;
  gap: 12px;
  overflow: visible;
  min-width: 0;
}

:deep(.common-setting-row__label) {
  flex: 0 0 168px;
  width: 168px;
  max-width: 168px;
  font-weight: 500;
  line-height: 1.35;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

:deep(.common-setting-row__control) {
  flex: 1 1 auto;
  display: flex;
  justify-content: flex-end;
  align-items: center;
  min-width: 180px;
  margin-left: auto;
  overflow: visible;
  position: relative;
}


:deep(.common-setting-row__control .el-switch) {
  flex-shrink: 0;
}

.usage-recon-fade-enter-active,
.usage-recon-fade-leave-active {
  transition:
    opacity 0.18s ease,
    transform 0.18s ease;
}

.usage-recon-fade-enter-from,
.usage-recon-fade-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}

@media (max-width: 900px) {
  .usage-recon-settings__grid {
    grid-template-columns: 1fr;
    column-gap: 0;
    row-gap: 12px;
  }

  :deep(.common-setting-row__label) {
    flex-basis: 148px;
    width: 148px;
    max-width: 148px;
  }
}

.usage-recon-detail {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding-bottom: 8px;
}

.usage-recon-detail__section-title {
  margin: 0 0 10px;
  font-size: 14px;
  font-weight: 600;
  color: var(--el-text-color-primary);
}

.usage-recon-detail__summary {
  padding: 12px 14px;
  border-radius: 8px;
  background: var(--el-fill-color-lighter);
  border: 1px solid var(--el-border-color-lighter);
}

.usage-recon-detail__summary-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px 16px;
}

.usage-recon-detail__summary-item {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.usage-recon-detail__summary-item--full {
  grid-column: 1 / -1;
}

.usage-recon-detail__summary-label {
  font-size: 12px;
  color: var(--el-text-color-secondary);
}

.usage-recon-detail__summary-value {
  font-size: 13px;
  font-weight: 500;
  color: var(--el-text-color-primary);
  line-height: 1.45;
  word-break: break-word;
}

.usage-recon-detail__summary-value--warn {
  color: var(--el-color-warning-dark-2);
}

.usage-recon-detail__amount-ok {
  color: var(--el-color-success);
  font-weight: 500;
}

.usage-recon-detail__muted {
  color: var(--el-text-color-secondary);
  font-weight: 400;
}

.usage-recon-detail__calls-table {
  width: 100%;
}

.usage-recon-detail__collapse {
  border: none;
}

.usage-recon-detail__collapse :deep(.el-collapse-item__header) {
  font-size: 13px;
  font-weight: 500;
  height: 40px;
  line-height: 40px;
}

.usage-recon-detail__methodology {
  margin: 0;
  padding-left: 1.2em;
  font-size: 13px;
  line-height: 1.65;
  color: var(--el-text-color-regular);
}

.usage-recon-detail__json {
  margin: 0;
  padding: 12px;
  max-height: 280px;
  overflow: auto;
  background: var(--el-fill-color-lighter);
  border-radius: 8px;
  font-size: 12px;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-word;
}

@media (max-width: 520px) {
  .usage-recon-detail__summary-grid {
    grid-template-columns: 1fr;
  }
}
</style>
