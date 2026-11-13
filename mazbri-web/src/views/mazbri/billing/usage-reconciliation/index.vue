<template>
  <div ref="pageRef" class="usage-recon-page">
    <div
      v-if="pagination.total === 0"
      class="usage-recon-compact-bar"
    >
      <div class="usage-recon-compact-bar__summary">
        <ArtSvgIcon
          icon="ri:calendar-check-line"
          class="usage-recon-compact-bar__icon"
        />
        <span class="usage-recon-compact-bar__title">
          {{ $t("ops.reconcile.calendar") }}
        </span>
        <span class="usage-recon-compact-bar__meta">
          {{ settingsSummaryLine }}
        </span>
      </div>
      <div class="usage-recon-compact-bar__actions">
        <ElSelect
          v-model="calendarDays"
          class="usage-recon-days-select"
          @change="loadCalendar"
        >
          <ElOption :value="7" :label="$t('ops.timeRange.presets.7d')" />
          <ElOption :value="30" :label="$t('ops.timeRange.presets.30d')" />
          <ElOption :value="90" :label="`90 ${$t('ops.units.days')}`" />
        </ElSelect>
      </div>
    </div>

    <OpsCard v-else :title="$t('ops.reconcile.calendar')">
      <template #extra>
        <div class="usage-recon-card-header-right">
          <span class="usage-recon-compact-bar__meta">
            {{ settingsSummaryLine }}
          </span>
          <ElSelect
            v-model="calendarDays"
            class="usage-recon-days-select"
            @change="loadCalendar"
          >
            <ElOption :value="7" :label="$t('ops.timeRange.presets.7d')" />
            <ElOption :value="30" :label="$t('ops.timeRange.presets.30d')" />
            <ElOption :value="90" :label="`90 ${$t('ops.units.days')}`" />
          </ElSelect>
        </div>
      </template>

      <div v-if="calendar.length > 0" class="calendar-grid">
        <div
          v-for="d in calendar"
          :key="d.day"
          class="calendar-cell"
          :class="`is-${d.status}`"
          :title="`${d.day}: ${statusLabel(d.status)}`"
        >
          <div class="day">{{ d.day.slice(5) }}</div>
          <div class="status">{{ statusLabel(d.status) }}</div>
        </div>
      </div>
      <div v-else class="usage-recon-calendar-note">
        {{ $t("ops.reconcile.noCalendarRecords") }}
      </div>
    </OpsCard>

    <ResourceTableShell
      v-model:columns="columns"
      :title="$t('billing.reconciliation.listTitle')"
      :show-header="props.active"
      :loading="loading"
      :data="items"
      :empty-text="emptyDescription"
      :pagination="artPagination"
      table-class="surface-panel-table usage-recon-table"
      @refresh="reloadAll"
      @selection-change="handleSelectionChange"
      @pagination:size-change="handlePageSizeChange"
      @pagination:current-change="handleCurrentPageChange"
    >
      <template #actions>
        <ResourceTableActionBar
          v-model="searchKeyword"
          v-model:filters="listFilters"
          :filter-catalog="reconciliationCatalog"
          :show-import="false"
          :show-export="true"
          :export-items="exportItems"
          :export-disabled="items.length === 0 || exporting"
          :export-loading="exporting"
          :export-label="$t('billing.actions.export')"
          :show-primary="canWriteBusiness"
          :primary-label="$t('billing.reconciliation.runAudit')"
          primary-icon="ri:play-circle-line"
          :primary-roles="[...ROLES_BIZ]"
          :show-delete="canWriteBusiness"
          :delete-total-count="items.length"
          :delete-selected-count="selectedRows.length"
          :delete-roles="[...ROLES_BIZ]"
          @search="handleSearch"
          @export="
            (command: string) =>
              handleExportCommand(command as 'page' | 'selected')
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
          <template #before-delete>
            <ElButton
              v-if="canWriteBusiness"
              v-roles="ROLES_BIZ"
              :loading="settingsLoading"
              @click="openPolicyDialog"
            >
              <template #icon>
                <ArtSvgIcon icon="ri:settings-3-line" />
              </template>
              {{ $t("billing.reconciliation.policySettings") }}
            </ElButton>
          </template>
        </ResourceTableActionBar>
      </template>

      <template #empty>
        <EmptyState compact :description="emptyDescription" />
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
            v-if="canWriteBusiness && canAcknowledge(row.status)"
            v-roles="ROLES_BIZ"
            link
            type="warning"
            @click="openResolve(row, 'acknowledged')"
          >
            {{ $t("billing.reconciliation.acknowledge") }}
          </ElButton>
          <ElButton
            v-if="canWriteBusiness && canResolve(row.status)"
            v-roles="ROLES_BIZ"
            link
            type="success"
            @click="openResolve(row, 'resolved')"
          >
            {{ $t("billing.reconciliation.resolve") }}
          </ElButton>
          <ElButton
            v-if="canWriteBusiness"
            v-roles="ROLES_BIZ"
            link
            type="danger"
            @click="handleDeleteRow(row)"
          >
            {{ $t("common.actions.delete") }}
          </ElButton>
        </TableActionRow>
      </template>
    </ResourceTableShell>

    <EditDialog
      v-model="policyDialogVisible"
      :title="$t('billing.reconciliation.policySettings')"
      width="540px"
      :confirm-text="$t('common.save')"
      :saving="settingsSaving"
      @confirm="handleSaveSettings"
    >
      <ElForm
        ref="policyFormRef"
        :model="policyForm"
        :rules="policyRules"
        label-width="140px"
        @submit.prevent
      >
        <ElFormItem
          :label="$t('billing.reconciliation.scheduledEnabled')"
          prop="enabled"
        >
          <ElSwitch v-model="policyForm.enabled" />
        </ElFormItem>
        <ElFormItem
          :label="$t('billing.reconciliation.intervalHours')"
          prop="intervalHours"
        >
          <div class="policy-inline-control">
            <ElInputNumber
              v-model="policyForm.intervalHours"
              :min="0.5"
              :max="720"
              :step="1"
              class="policy-number-input"
            />
            <span class="policy-unit">{{ $t("ops.units.hours") }}</span>
          </div>
        </ElFormItem>
        <ElFormItem
          :label="$t('billing.reconciliation.lossRatioThreshold')"
          prop="thresholdPercent"
        >
          <div class="policy-inline-control">
            <ElInputNumber
              v-model="policyForm.thresholdPercent"
              :min="0"
              :max="100"
              :step="0.5"
              :precision="2"
              class="policy-number-input"
            />
            <span class="policy-unit">%</span>
          </div>
        </ElFormItem>
        <ElFormItem
          :label="$t('billing.reconciliation.usePreviousDay')"
          prop="usePreviousDay"
        >
          <ElSwitch v-model="policyForm.usePreviousDay" />
        </ElFormItem>
        <ElFormItem
          :label="$t('billing.reconciliation.includeAmount')"
          prop="includeAmount"
        >
          <ElSwitch v-model="policyForm.includeAmount" />
        </ElFormItem>
        <ElFormItem
          :label="$t('billing.reconciliation.warnBeforeBill')"
          prop="warnBeforeBill"
        >
          <ElSwitch v-model="policyForm.warnBeforeBill" />
        </ElFormItem>
        <ElFormItem
          :label="$t('billing.reconciliation.alertOnFlagged')"
          prop="alertOnFlagged"
        >
          <ElSwitch v-model="policyForm.alertOnFlagged" />
        </ElFormItem>
        <ElFormItem
          :label="$t('billing.reconciliation.alertWebhook')"
          prop="webhookUrl"
        >
          <ElInput
            v-model="policyForm.webhookUrl"
            clearable
            :disabled="!policyForm.alertOnFlagged"
            :placeholder="$t('billing.reconciliation.alertWebhookPlaceholder')"
          />
        </ElFormItem>
      </ElForm>
    </EditDialog>

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
            {{ currentRow.resolution_note || "—" }}
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
            <div
              class="usage-recon-detail__summary-item usage-recon-detail__summary-item--full"
            >
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
            <div
              class="usage-recon-detail__summary-item usage-recon-detail__summary-item--full"
            >
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

        <section v-if="detailView?.amount" class="usage-recon-detail__section">
          <h4 class="usage-recon-detail__section-title">
            {{ $t("billing.reconciliation.detail.amountTitle") }}
          </h4>
          <ElDescriptions :column="1" border size="small">
            <ElDescriptionsItem
              :label="$t('billing.reconciliation.detail.usageAmount')"
            >
              {{ formatMoney(detailView.amount.usageBillable) }}
            </ElDescriptionsItem>
            <ElDescriptionsItem
              :label="$t('billing.reconciliation.detail.billItemsTotal')"
            >
              {{ formatMoney(detailView.amount.billItemsTotal) }}
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
                {{ formatMoney(detailView.amount.diff) }}
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
              <li v-for="(line, idx) in detailView.methodologyLines" :key="idx">
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
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import type { ListFilter } from "@/types/list-query";
import { reconciliationCatalog } from "@/list-catalog/common-resources";
import { ElMessage, ElMessageBox } from "element-plus";
import type { FormInstance, FormRules } from "element-plus";
import { useRouter } from "vue-router";
import OpsCard from "@/components/ops/OpsCard.vue";
import EmptyState from "@/components/common/empty-state/index.vue";
import ResourceTableActionBar from "@/components/common/resource-table-action-bar/index.vue";
import ResourceTableShell from "@/components/common/resource-table-shell/index.vue";
import CommonDrawerHeader from "@/components/common/drawer-header/index.vue";
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
import {
  fetchReconcileCalendar,
  type ReconcileCalendarDay,
} from "@/api/billing-ops";
import { $t } from "@/locales";
import { ROLES_BIZ } from "@/utils/ops/roles";
import { useUserStore } from "@/store/modules/user";
import { runWithLoading } from "@/composables/useAsyncState";
import { useTableSelection } from "@/composables/useTableSelection";
import { logger } from "@/utils/sys";
import { formatMoney, formatDate } from "@/utils/format";
import { downloadCsv, serializeRawCsv } from "@/utils/csv";
import type { ColumnOption, PaginationConfig } from "@/types/component";
import { buildCallsRoute } from "@/utils/ops/opsRoutes";
import {
  hoursToSeconds,
  percentToRatio,
  ratioToPercent,
  secondsToHours,
} from "./reconcilePolicyHelpers";

defineOptions({ name: "UsageReconciliation" });

const props = withDefaults(
  defineProps<{
    active?: boolean;
  }>(),
  {
    active: true,
  },
);

const router = useRouter();
const userStore = useUserStore();
const canWriteBusiness = computed(() => userStore.canWriteBusiness);
const pageRef = ref<HTMLElement | null>(null);
const loading = ref(false);
const runLoading = ref(false);
const exporting = ref(false);
const settingsLoading = ref(false);
const settingsSaving = ref(false);

const calendarDays = ref(30);
const calendar = ref<ReconcileCalendarDay[]>([]);

const policyDialogVisible = ref(false);
const policyFormRef = ref<FormInstance>();
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

const policyForm = reactive({
  enabled: false,
  intervalHours: 24,
  thresholdPercent: 5,
  usePreviousDay: true,
  includeAmount: true,
  warnBeforeBill: true,
  alertOnFlagged: false,
  webhookUrl: "",
});

const policyRules: FormRules = {
  intervalHours: [
    {
      required: true,
      validator: (_rule, value: unknown, callback) => {
        const n = Number(value);
        if (!Number.isFinite(n) || n < 0.5) {
          callback(new Error($t("billing.reconciliation.intervalHoursInvalid")));
          return;
        }
        callback();
      },
      trigger: ["blur", "change"],
    },
  ],
  thresholdPercent: [
    {
      required: true,
      validator: (_rule, value: unknown, callback) => {
        const n = Number(value);
        if (!Number.isFinite(n) || n < 0 || n > 100) {
          callback(new Error($t("billing.reconciliation.thresholdInvalid")));
          return;
        }
        callback();
      },
      trigger: ["blur", "change"],
    },
  ],
  webhookUrl: [
    {
      validator: (_rule, value: unknown, callback) => {
        if (!policyForm.alertOnFlagged) {
          callback();
          return;
        }
        const trimmed = String(value ?? "").trim();
        if (!trimmed || !/^https?:\/\/\S+$/i.test(trimmed)) {
          callback(new Error($t("billing.reconciliation.alertWebhookRequired")));
          return;
        }
        callback();
      },
      trigger: "blur",
    },
  ],
};

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
      void reloadAll();
    },
    allDeleteConfirmMsg: (count) =>
      $t("billing.reconciliation.deleteConfirm.all", { count }),
    selectedDeleteConfirmMsg: (count) =>
      $t("billing.reconciliation.deleteConfirm.selected", { count }),
    actionLabel: $t("common.actions.delete"),
  });
const columns = ref<ColumnOption<ReconciliationDiscrepancy>[]>([
  { type: "selection", width: 48 },
  {
    prop: "reconciliation_date",
    label: $t("billing.reconciliation.columns.date"),
    minWidth: 96,
    useSlot: true,
  },
  {
    prop: "billable_call_count",
    label: $t("billing.reconciliation.columns.oltp"),
    minWidth: 90,
    useSlot: true,
  },
  {
    prop: "gateway_call_count",
    label: $t("billing.reconciliation.columns.gateway"),
    minWidth: 90,
    useSlot: true,
  },
  {
    prop: "upstream_preagg_call_count",
    label: $t("billing.reconciliation.columns.olap"),
    minWidth: 96,
    useSlot: true,
  },
  {
    prop: "discrepancy_ratio",
    label: $t("billing.reconciliation.columns.ratio"),
    minWidth: 84,
    useSlot: true,
  },
  {
    prop: "status",
    label: $t("billing.reconciliation.columns.status"),
    minWidth: 84,
    useSlot: true,
  },
  {
    prop: "created_at",
    label: $t("common.fields.createdAt"),
    minWidth: 140,
    useSlot: true,
  },
  {
    prop: "operations",
    label: $t("common.fields.operations"),
    width: 300,
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
  if (Number.isNaN(n)) return String(value ?? "—");
  return `${(n * 100).toFixed(2)}%`;
};

const formatCount = (value: string | number | null | undefined) => {
  const n = Number(value ?? 0);
  if (!Number.isFinite(n)) return String(value ?? "—");
  return new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 0 }).format(n);
};

const asRecord = (value: unknown): Record<string, unknown> | null => {
  if (value && typeof value === "object" && !Array.isArray(value)) {
    return value as Record<string, unknown>;
  }
  return null;
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
    row.status === "error" ? (asString(details.error) ?? "—") : null;
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
    const usageBillable = asString(amountObj.usage_billable_amount) ?? "0";
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
  const intervalText = `${secondsToHours(settingsForm.schedule.interval_secs)} ${$t("ops.units.hours")}`;
  const threshold = $t("billing.reconciliation.summaryThreshold", {
    value: formatRatio(settingsForm.loss_ratio_threshold),
  });
  const last = settingsForm.last_scheduled_run_at
    ? formatDate(settingsForm.last_scheduled_run_at)
    : $t("billing.reconciliation.neverRun");
  return `${scheduled} (${intervalText}) · ${threshold} · ${last}`;
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

const loadCalendar = async () => {
  if (!canWriteBusiness.value) return;
  try {
    calendar.value = (await fetchReconcileCalendar(calendarDays.value)) ?? [];
  } catch (error) {
    logger.error("Failed to load reconcile calendar:", error);
    ElMessage.error($t("ops.reconcile.loadFailed"));
  }
};

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

const reloadAll = async () => {
  await Promise.all([loadData(), loadCalendar()]);
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

const syncPolicyFormFromSettings = () => {
  policyForm.enabled = settingsForm.schedule.enabled;
  policyForm.intervalHours = secondsToHours(settingsForm.schedule.interval_secs);
  policyForm.thresholdPercent = ratioToPercent(
    settingsForm.loss_ratio_threshold,
  );
  policyForm.usePreviousDay = settingsForm.schedule.use_previous_day;
  policyForm.includeAmount = settingsForm.sources.include_amount_recon;
  policyForm.warnBeforeBill =
    settingsForm.warn_on_open_discrepancies_before_bill;
  policyForm.alertOnFlagged = settingsForm.alert.on_flagged;
  policyForm.webhookUrl = settingsForm.alert.webhook_url ?? "";
};

const openPolicyDialog = () => {
  syncPolicyFormFromSettings();
  policyFormRef.value?.clearValidate();
  policyDialogVisible.value = true;
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
    await reloadAll();
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
  if (!row?.id) return;
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
    await reloadAll();
  } catch (error) {
    if (error !== "cancel") {
      logger.error("Failed to delete reconciliation discrepancy:", error);
    }
  }
};

const openDetail = (row: ReconciliationDiscrepancy) => {
  if (!row?.id) return;
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
    await reloadAll();
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
  const start = new Date(`${day}T00:00:00+08:00`).getTime();
  const end = new Date(`${day}T23:59:59.999+08:00`).getTime();
  void router.push(
    buildCallsRoute({
      tab: mode,
      start: Math.floor(start / 1000),
      end: Math.floor(end / 1000),
    }),
  );
};

const loadSettings = async () => {
  settingsLoading.value = true;
  try {
    const res = await fetchReconciliationSettings();
    Object.assign(settingsForm, res);
    syncPolicyFormFromSettings();
  } catch (error) {
    logger.error("Failed to load reconciliation settings:", error);
  } finally {
    settingsLoading.value = false;
  }
};

const handleSaveSettings = async () => {
  const valid = await policyFormRef.value?.validate().catch(() => false);
  if (!valid) return;
  settingsSaving.value = true;
  try {
    const payload: ReconciliationSettings = {
      ...settingsForm,
      schedule: {
        ...settingsForm.schedule,
        enabled: policyForm.enabled,
        interval_secs: hoursToSeconds(policyForm.intervalHours),
        use_previous_day: policyForm.usePreviousDay,
      },
      alert: {
        on_flagged: policyForm.alertOnFlagged,
        webhook_url: policyForm.webhookUrl.trim(),
      },
      sources: {
        ...settingsForm.sources,
        include_amount_recon: policyForm.includeAmount,
      },
      loss_ratio_threshold: percentToRatio(policyForm.thresholdPercent),
      warn_on_open_discrepancies_before_bill: policyForm.warnBeforeBill,
    };
    const saved = await updateReconciliationSettings(payload);
    Object.assign(settingsForm, saved);
    syncPolicyFormFromSettings();
    policyDialogVisible.value = false;
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
  void reloadAll();
});
</script>

<style scoped lang="scss">
.usage-recon-page {
  display: flex;
  flex-direction: column;
  gap: var(--ops-section-gap, 16px);
  flex: 1;
  min-width: 0;
  min-height: 0;
}

.usage-recon-compact-bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 8px 16px;
  padding: 10px 14px;
  border-radius: var(--art-card-radius);
  border: 1px solid var(--art-card-border, var(--el-border-color-lighter));
  background: var(--el-bg-color);
}

.usage-recon-compact-bar__summary {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.usage-recon-compact-bar__icon {
  color: var(--el-color-primary);
  font-size: 16px;
  flex-shrink: 0;
}

.usage-recon-compact-bar__title {
  font-size: 13px;
  font-weight: 600;
  color: var(--el-text-color-primary);
}

.usage-recon-compact-bar__meta {
  font-size: 12px;
  color: var(--el-text-color-secondary);
}

.usage-recon-compact-bar__actions,
.usage-recon-card-header-right {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  flex-shrink: 0;
}

.usage-recon-days-select {
  width: 112px;
  --el-select-width: 112px;
}

.calendar-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(76px, 1fr));
  gap: 8px;
}

.calendar-cell {
  padding: 8px;
  border-radius: var(--art-form-control-radius);
  border: 1px solid var(--el-border-color-lighter);
  background: var(--el-fill-color-light);
  text-align: center;
  font-size: 12px;
}

.calendar-cell.is-ok {
  background: var(--el-color-success-light-9);
  border-color: var(--el-color-success-light-7);
}

.calendar-cell.is-open,
.calendar-cell.is-flagged,
.calendar-cell.is-acknowledged {
  background: var(--el-color-warning-light-9);
  border-color: var(--el-color-warning-light-7);
}

.calendar-cell.is-error {
  background: var(--el-color-danger-light-9);
  border-color: var(--el-color-danger-light-7);
}

.calendar-cell .day {
  font-weight: 600;
  color: var(--el-text-color-primary);
}

.calendar-cell .status {
  margin-top: 4px;
  color: var(--el-text-color-secondary);
}

.usage-recon-calendar-note {
  font-size: 12px;
  color: var(--el-text-color-secondary);
}

.policy-inline-control {
  display: inline-flex;
  align-items: center;
  gap: 8px;
}

.policy-number-input {
  width: 160px;
}

.policy-unit {
  font-size: 13px;
  color: var(--el-text-color-regular);
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
  border-radius: var(--art-card-radius);
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
  border-radius: var(--art-form-control-radius);
  font-size: 12px;
  line-height: 1.5;
}
</style>
