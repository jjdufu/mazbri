<template>
  <ControlPanelPageShell
    :title="$t('menus.mazbri.billList')"
    icon="ri:file-list-3-line"
  >
    <div
      ref="pageRef"
      class="bill-ops-page"
      style="display: flex; flex-direction: column; flex: 1; min-height: 0"
    >
      <ResourceTablePageShell
          v-model:columns="columns"
          v-model="searchKeyword"
          v-model:filters="listFilters"
          :filter-catalog="billsCatalog"
          :title="$t('billing.list.title')"
          :loading="loading"
          :data="bills"
          :empty-text="emptyDescription"
          :pagination="artPagination"
          :show-import="false"
          :show-export="true"
          :export-items="exportItems"
          :export-disabled="bills.length === 0 || exporting"
          :export-loading="exporting"
          :export-label="$t('billing.actions.export')"
          :show-primary="true"
          :primary-label="$t('billing.actions.generate')"
          primary-icon="ri:calendar-event-line"
          :primary-disabled="false"
          :primary-roles="['R_ADMIN', 'R_FINANCE']"
          :show-delete="true"
          :delete-total-count="bills.length"
          :delete-selected-count="selectedRows.length"
          :delete-roles="['R_ADMIN']"
          :fit="false"
          table-layout="auto"
          table-class="surface-panel-table bill-list-table"
          @refresh="loadData"
          @selection-change="handleSelectionChange"
          @pagination:size-change="handlePageSizeChange"
          @pagination:current-change="handleCurrentPageChange"
          @search="handleSearch"
          @export="(command: 'page' | 'selected') => handleExportCommand(command)"
          @primary="handleOpenGenerateDialog"
          @delete="(type: 'all' | 'selected') => handleDeleteCommand(type, bills)"
        >
          <template #tenant_name="{ row }">
            <div class="tenant-name-cell">
              <span class="tenant-name-text">{{ row.tenant_name }}</span>
            </div>
          </template>
          <template #tenant_code="{ row }">
            <div class="tenant-code-cell">
              <span class="tenant-code-text">{{ row.tenant_code }}</span>
            </div>
          </template>
          <template #bill_kind="{ row }">
            <ElTag :type="row.bill_kind === 'upstream' ? 'warning' : 'success'">
              {{ getBillKindLabel(row.bill_kind) }}
            </ElTag>
          </template>
          <template #period_start="{ row }">
            {{ formatDateOnly(row.period_start) }}
          </template>
          <template #period_end="{ row }">
            {{ formatDateOnly(row.period_end) }}
          </template>
          <template #total="{ row }">
            <span class="font-medium">
              {{ formatAmount(row.total) }}
            </span>
          </template>
          <template #status="{ row }">
            <ElTag :type="statusTagType(row.status)" size="small">
              {{ getStatusLabel(row.status) }}
            </ElTag>
          </template>
          <template #operations="{ row }">
            <TableActionRow>
              <ElButton link type="primary" @click="handleView(row)">{{
                $t("common.actions.details")
              }}</ElButton>
              <ElButton
                v-roles="['R_ADMIN', 'R_FINANCE']"
                v-if="canMarkPaid(row.status)"
                link
                type="success"
                @click="handleUpdateStatus(row, 'paid')"
              >
                {{ $t("billing.actions.markPaid") }}
              </ElButton>
              <ElButton
                v-roles="['R_ADMIN', 'R_FINANCE']"
                v-if="canMarkOverdue(row.status)"
                link
                type="warning"
                @click="handleUpdateStatus(row, 'overdue')"
              >
                {{ $t("billing.actions.markOverdue") }}
              </ElButton>
              <ElButton
                v-roles="['R_ADMIN']"
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

    <AdaptiveDrawer v-model="detailVisible" size="960px" destroy-on-close>
      <template #header>
        <CommonDrawerHeader :title="$t('billing.detail.title')" />
      </template>

      <ElSkeleton :loading="detailLoading" animated>
        <template #default>
          <div v-if="currentBill" class="bill-detail-layout">
            <div class="bill-detail-toolbar">
              <ElButton
                size="large"
                type="primary"
                :disabled="!currentBill"
                @click="handleExportDetail"
              >
                <template #icon
                  ><ArtSvgIcon icon="ri:download-line"
                /></template>
                {{ $t("billing.detail.download") }}
              </ElButton>
            </div>
            <ElCard shadow="never" class="bill-detail-overview-card">
              <CommonDetailHero
                :eyebrow="$t('billing.detail.overview')"
                :title="getBillDisplayName(currentBill)"
                :subtitle="`${currentBill.tenant_code || currentBill.party_code || '-'} · ${currentBill.tenant_id || currentBill.party_id || '-'}`"
                :amount="formatAmount(currentBill.total)"
                :meta="formatDate(currentBill.created_at)"
              />

              <ElDescriptions
                :column="3"
                border
                class="bill-detail-descriptions"
              >
                <ElDescriptionsItem
                  :label="$t('billing.labels.billId')"
                  :span="2"
                >
                  {{ currentBill.id }}
                </ElDescriptionsItem>
                <ElDescriptionsItem :label="$t('billing.labels.billKind')">
                  {{ getBillKindLabel(currentBill.bill_kind) }}
                </ElDescriptionsItem>
                <ElDescriptionsItem :label="$t('billing.labels.period')">
                  {{ formatDateOnly(currentBill.period_start) }} ~
                  {{ formatDateOnly(currentBill.period_end) }}
                </ElDescriptionsItem>
                <ElDescriptionsItem :label="$t('billing.labels.dueDate')">
                  {{ formatDate(currentBill.due_date) }}
                </ElDescriptionsItem>
                <ElDescriptionsItem :label="$t('billing.labels.total')">
                  {{ formatAmount(currentBill.total) }}
                </ElDescriptionsItem>
                <ElDescriptionsItem :label="$t('billing.labels.totalCalls')">
                  {{ currentBill.total_calls ?? 0 }}
                </ElDescriptionsItem>
                <ElDescriptionsItem :label="$t('billing.labels.dataVolume')">
                  {{ formatBytes(currentBill.total_data_bytes ?? 0) }}
                </ElDescriptionsItem>
                <ElDescriptionsItem :label="$t('common.fields.createdAt')">
                  {{ formatDate(currentBill.created_at) }}
                </ElDescriptionsItem>
                <ElDescriptionsItem :label="$t('common.fields.updatedAt')">
                  {{ formatDate(currentBill.updated_at) }}
                </ElDescriptionsItem>
              </ElDescriptions>
            </ElCard>

            <ElCard shadow="never" class="bill-detail-items-card">
              <template #header>
                <SurfaceCardHeader
                  align="start"
                  :title="$t('billing.detail.itemsTitle')"
                  :description="
                    $t('billing.detail.itemsCount', { count: billItems.length })
                  "
                  :meta="formatAmount(currentBill.total)"
                />
              </template>

              <ArtTable
                :data="billItems"
                :columns="billItemColumns"
                border
                stripe
                :empty-text="$t('billing.detail.emptyItems')"
                class="bill-detail-items-table"
              >
                <template #unit_price="{ row }">
                  {{ formatAmount(row.unit_price) }}
                </template>
                <template #total="{ row }">
                  {{ formatAmount(row.total) }}
                </template>
              </ArtTable>
            </ElCard>

            <ElCard shadow="never" class="bill-detail-items-card">
              <template #header>
                <SurfaceCardHeader
                  align="start"
                  :title="$t('billing.adjustments.title')"
                >
                  <template #actions>
                    <ElButton
                      v-roles="['R_ADMIN', 'R_FINANCE']"
                      type="primary"
                      size="small"
                      @click="openCreateAdjustment"
                    >
                      {{ $t("billing.adjustments.add") }}
                    </ElButton>
                  </template>
                </SurfaceCardHeader>
              </template>
              <ArtTable
                :data="billAdjustments"
                :columns="adjustmentColumns"
                border
                stripe
                :loading="adjustmentsLoading"
                :empty-text="$t('billing.adjustments.empty')"
                class="bill-detail-items-table"
              >
                <template #amount="{ row }">
                  {{ formatAmount(row.amount) }}
                </template>
                <template #status="{ row }">
                  <ElTag
                    :type="row.status === 'void' ? 'info' : 'success'"
                    size="small"
                  >
                    {{
                      row.status === "void"
                        ? $t("billing.adjustments.statuses.void")
                        : $t("billing.adjustments.statuses.posted")
                    }}
                  </ElTag>
                </template>
                <template #created_at="{ row }">
                  {{ formatDate(row.created_at) }}
                </template>
                <template #operations="{ row }">
                  <ElButton
                    v-if="row.status === 'posted'"
                    v-roles="['R_ADMIN', 'R_FINANCE']"
                    link
                    type="danger"
                    @click="handleVoidAdjustment(row)"
                  >
                    {{ $t("billing.adjustments.void") }}
                  </ElButton>
                  <span v-else>-</span>
                </template>
              </ArtTable>
            </ElCard>
          </div>
        </template>
      </ElSkeleton>
    </AdaptiveDrawer>

    <EditDialog
      v-model="adjustmentDialogVisible"
      :title="$t('billing.adjustments.addTitle')"
      width="480px"
      :confirm-text="$t('common.confirm')"
      :saving="adjustmentSaving"
      @confirm="handleCreateBillAdjustment"
    >
      <ElForm label-position="top" @submit.prevent>
        <ElFormItem :label="$t('billing.adjustments.type')">
          <ElSelect v-model="adjustmentForm.adjustment_type" style="width: 100%">
            <ElOption
              :label="$t('billing.adjustments.types.credit')"
              value="credit"
            />
            <ElOption
              :label="$t('billing.adjustments.types.debit')"
              value="debit"
            />
            <ElOption
              :label="$t('billing.adjustments.types.correction')"
              value="correction"
            />
            <ElOption
              :label="$t('billing.adjustments.types.other')"
              value="other"
            />
          </ElSelect>
        </ElFormItem>
        <ElFormItem :label="$t('billing.adjustments.amount')">
          <ElInput
            v-model="adjustmentForm.amount"
            :placeholder="$t('billing.adjustments.amountPlaceholder')"
          />
        </ElFormItem>
        <ElFormItem :label="$t('billing.adjustments.reason')">
          <ElInput
            v-model="adjustmentForm.reason"
            type="textarea"
            :rows="3"
            maxlength="500"
            show-word-limit
            :placeholder="$t('billing.adjustments.reasonPlaceholder')"
          />
        </ElFormItem>
      </ElForm>
    </EditDialog>

    <EditDialog
      v-model="generateDialogVisible"
      :title="$t('billing.generateDialog.title')"
      width="560px"
      :confirm-text="$t('billing.generateDialog.confirm')"
      :saving="generating"
      @confirm="handleGenerateBill"
    >
      <ElForm
        ref="generateFormRef"
        :model="generateForm"
        :rules="generateRules"
        label-width="100px"
      >
        <ElFormItem
          :label="$t('billing.generateDialog.billKind')"
          prop="bill_kind"
        >
          <ElSelect v-model="generateForm.bill_kind" style="width: 100%">
            <ElOption
              :label="$t('billing.billKinds.customer')"
              value="customer"
            />
            <ElOption
              :label="$t('billing.billKinds.upstream')"
              value="upstream"
            />
          </ElSelect>
        </ElFormItem>
        <ElFormItem
          v-if="generateForm.bill_kind === 'customer'"
          :label="$t('billing.generateDialog.tenant')"
          prop="tenant_code"
        >
          <ElSelect
            v-model="generateForm.tenant_code"
            :placeholder="$t('billing.generateDialog.tenantPlaceholder')"
            style="width: 100%"
            filterable
            :loading="tenantsLoading"
          >
            <ElOption
              v-for="tenant in tenants"
              :key="tenant.tenant_id"
              :label="`${tenant.name} (${tenant.tenant_code})`"
              :value="tenant.tenant_code"
            />
          </ElSelect>
        </ElFormItem>
        <template v-else>
          <ElFormItem
            :label="$t('billing.generateDialog.upstreamScopeType')"
            prop="upstream_scope_type"
          >
            <ElSelect
              v-model="generateForm.upstream_scope_type"
              style="width: 100%"
            >
              <ElOption
                :label="$t('billing.generateDialog.upstreamChannel')"
                value="channel"
              />
              <ElOption
                :label="$t('billing.generateDialog.upstreamSupplier')"
                value="supplier"
              />
            </ElSelect>
          </ElFormItem>
          <ElFormItem
            :label="$t('billing.generateDialog.upstreamScope')"
            prop="upstream_scope_id"
          >
            <ElSelect
              v-if="generateForm.upstream_scope_type === 'channel'"
              v-model="generateForm.upstream_scope_id"
              :placeholder="
                $t('billing.generateDialog.upstreamChannelPlaceholder')
              "
              style="width: 100%"
              filterable
              :loading="channelsLoading"
            >
              <ElOption
                v-for="channel in channels"
                :key="channel.id"
                :label="`${channel.name} (${channel.channel_code})`"
                :value="channel.id"
              />
            </ElSelect>
            <ElSelect
              v-else
              v-model="generateForm.upstream_scope_id"
              :placeholder="
                $t('billing.generateDialog.upstreamSupplierPlaceholder')
              "
              style="width: 100%"
              filterable
              :loading="suppliersLoading"
            >
              <ElOption
                v-for="supplier in suppliers"
                :key="supplier.id"
                :label="`${supplier.name} (${supplier.supplier_code})`"
                :value="supplier.id"
              />
            </ElSelect>
          </ElFormItem>
        </template>
        <ElFormItem
          :label="$t('billing.generateDialog.periodRange')"
          prop="period_range"
        >
          <DatePicker
            v-model="generateForm.period_range"
            type="daterange"
            :start-placeholder="
              $t('billing.generateDialog.periodStartPlaceholder')
            "
            :end-placeholder="$t('billing.generateDialog.periodEndPlaceholder')"
            format="YYYY-MM-DD"
            value-format="YYYY-MM-DD"
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
import { billsCatalog } from "@/list-catalog/common-resources";
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import { ElMessage, ElMessageBox } from "element-plus";
import type { FormInstance, FormRules } from "element-plus";
import type {
  Bill,
  BillLineItem,
  BillListQuery,
  GenerateBillRequest,
} from "@/types/api/billing";
import {
  createBillAdjustment,
  deleteBill,
  fetchBillAdjustments,
  fetchBillById,
  fetchBillList,
  generateBill,
  generateBillAsync,
  updateBillStatus,
  voidBillAdjustment,
  type BillAdjustment,
} from "@/api/billing";

import { fetchTenantList } from "@/api/tenant";
import type { Tenant } from "@/types/api/tenant";
import { fetchChannels, type Channel } from "@/api/route-management";
import { fetchSuppliers, type Supplier } from "@/api/supplier";
import { $t } from "@/locales";
import ResourceTablePageShell from "@/components/common/resource-table-page-shell/index.vue";
import CommonDetailHero from "@/components/common/detail-hero/index.vue";
import SurfaceCardHeader from "@/components/common/surface-card-header/index.vue";
import CommonDrawerHeader from "@/components/common/drawer-header/index.vue";
import EditDialog from "@/components/common/EditDialog.vue";
import AdaptiveDrawer from "@/components/common/AdaptiveDrawer.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { useAdaptiveTableColumns } from "@/composables/useAdaptiveTableColumns";
import { getHttpErrorRawMessage } from "@/utils/http/error";
import { logger } from "@/utils/sys";
import { formatDate, formatAmount, formatBytes } from "@/utils/format";
import { downloadCsv, serializeRawCsv } from "@/utils/csv";
import { downloadBinary, createTarArchive, gzipBytes } from "@/utils/archive";
import type { ColumnOption, PaginationConfig } from "@/types/component";
import { useTableSelection } from "@/composables/useTableSelection";
import {
  canMarkPaid,
  canMarkOverdue,
  getStatusLabel,
  statusTagType,
} from "./statusHelpers";
defineOptions({ name: "BillList" });
const pageRef = ref<HTMLElement | null>(null);
const loading = ref(false);
const bills = ref<Bill[]>([]);
const tenants = ref<Tenant[]>([]);
const channels = ref<Channel[]>([]);
const suppliers = ref<Supplier[]>([]);
const detailVisible = ref(false);
const detailLoading = ref(false);
const currentBill = ref<Bill | null>(null);
const billItems = ref<BillLineItem[]>([]);
const billAdjustments = ref<BillAdjustment[]>([]);
const adjustmentsLoading = ref(false);
const adjustmentDialogVisible = ref(false);
const adjustmentSaving = ref(false);
const adjustmentForm = reactive({
  adjustment_type: "credit",
  amount: "",
  reason: "",
});
const adjustmentColumns = ref<ColumnOption<BillAdjustment>[]>([
  {
    prop: "adjustment_type",
    label: $t("billing.adjustments.type"),
    minWidth: 100,
  },
  {
    prop: "amount",
    label: $t("billing.adjustments.amount"),
    minWidth: 110,
    useSlot: true,
  },
  {
    prop: "reason",
    label: $t("billing.adjustments.reason"),
    minWidth: 112,
  },
  {
    prop: "status",
    label: $t("billing.adjustments.status"),
    width: 90,
    useSlot: true,
  },
  {
    prop: "created_at",
    label: $t("common.fields.createdAt"),
    minWidth: 100,
    useSlot: true,
  },
  {
    prop: "operations",
    label: $t("common.fields.operations"),
    width: 90,
    fixed: "right",
    useSlot: true,
  },
]);
const generateDialogVisible = ref(false);
const generateFormRef = ref<FormInstance>();
const tenantsLoading = ref(false);
const channelsLoading = ref(false);
const suppliersLoading = ref(false);
const generating = ref(false);
const exporting = ref(false);
const columns = ref<ColumnOption<Bill>[]>([]);
const billColumns: ColumnOption<Bill>[] = [
  { type: "selection", width: 55 },
  {
    prop: "tenant_name",
    label: $t("billing.columns.tenantName"),
    minWidth: 150,
    useSlot: true,
  },
  {
    prop: "tenant_code",
    label: $t("billing.columns.tenantCode"),
    minWidth: 120,
    useSlot: true,
  },
  {
    prop: "bill_kind",
    label: $t("billing.columns.billKind"),
    minWidth: 96,
    useSlot: true,
  },
  {
    prop: "period_start",
    label: $t("billing.columns.periodStart"),
    minWidth: 100,
    useSlot: true,
  },
  {
    prop: "period_end",
    label: $t("billing.columns.periodEnd"),
    minWidth: 100,
    useSlot: true,
  },
  {
    prop: "total",
    label: $t("billing.columns.total"),
    minWidth: 100,
    useSlot: true,
  },
  {
    prop: "status",
    label: $t("billing.columns.status"),
    minWidth: 110,
    useSlot: true,
  },
  {
    prop: "operations",
    label: $t("common.fields.operations"),
    width: 220,
    fixed: "right",
    useSlot: true,
  },
];
const handleUpdateStatus = async (
  row: Bill,
  status: "paid" | "overdue" | "processing" | "cancelled",
) => {
  try {
    await ElMessageBox.confirm(
      $t("billing.statusConfirm.message", {
        status: getStatusLabel(status),
        name: row.tenant_name || row.id,
      }),
      $t("billing.statusConfirm.title"),
      { type: "warning" },
    );
    await updateBillStatus(row.id, status);
    await loadData();
    if (currentBill.value?.id === row.id) {
      await handleView(row);
    }
  } catch (e) {
    if (e !== "cancel") {
      logger.error("update bill status failed", e);
    }
  }
};
const billItemColumns = ref<ColumnOption<BillLineItem>[]>([
  {
    prop: "description",
    label: $t("billing.itemColumns.description"),
    minWidth: 140,
  },
  { prop: "quantity", label: $t("billing.itemColumns.quantity"), width: 100 },
  {
    prop: "total_units",
    label: $t("billing.itemColumns.totalUnits"),
    width: 100,
  },
  {
    prop: "unit_price",
    label: $t("billing.itemColumns.unitPrice"),
    width: 120,
    useSlot: true,
  },
  {
    prop: "total",
    label: $t("billing.itemColumns.total"),
    width: 120,
    useSlot: true,
  },
]);
const searchForm = reactive<BillListQuery>({
  q: "",
});
const listFilters = ref<ListFilter[]>([]);
const searchKeyword = computed<string>({
  get: () => searchForm.q ?? "",
  set: (value) => {
    searchForm.q = value;
  },
});
const generateForm = reactive<{
  bill_kind: "customer" | "upstream";
  tenant_code: string;
  upstream_scope_type: "channel" | "supplier";
  upstream_scope_id: string;
  period_range: [] | [string, string];
}>({
  bill_kind: "customer",
  tenant_code: "",
  upstream_scope_type: "channel",
  upstream_scope_id: "",
  period_range: [],
});
const generateRules: FormRules = {
  bill_kind: [
    {
      required: true,
      message: $t("billing.validation.billKindRequired"),
      trigger: "change",
    },
  ],
  tenant_code: [
    {
      validator: (_rule, value, callback) => {
        if (
          generateForm.bill_kind === "customer" &&
          !String(value ?? "").trim()
        ) {
          callback(new Error($t("billing.validation.tenantRequired")));
          return;
        }
        callback();
      },
      trigger: "change",
    },
  ],
  upstream_scope_id: [
    {
      validator: (_rule, value, callback) => {
        if (
          generateForm.bill_kind === "upstream" &&
          !String(value ?? "").trim()
        ) {
          callback(new Error($t("billing.validation.upstreamScopeRequired")));
          return;
        }
        callback();
      },
      trigger: "change",
    },
  ],
  period_range: [
    {
      required: true,
      message: $t("billing.validation.periodRangeRequired"),
      trigger: "change",
    },
  ],
};
const hasActiveSearch = computed(
  () => Boolean(searchForm.q?.trim()) || listFilters.value.length > 0,
);
const emptyDescription = computed(() =>
  hasActiveSearch.value
    ? $t("billing.empty.noMatch")
    : $t("billing.empty.noData"),
);
const pagination = reactive({
  page: 1,
  limit: 10,
  total: 0,
});
const artPagination = computed<PaginationConfig>(() => ({
  current: pagination.page,
  size: pagination.limit,
  total: pagination.total,
}));
const exportItems = computed(() => [
  {
    command: "page",
    label: $t("billing.actions.exportPage"),
    disabled: bills.value.length === 0,
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
  useTableSelection<Bill>({
    deleteApi: (row) => deleteBill(row.id),
    getItemName: (row) => row.tenant_name,
    getItemId: (row) => row.id,
    onDeleteSuccess: () => loadData(),
    allDeleteConfirmMsg: (count) => $t("billing.deleteConfirm.all", { count }),
    selectedDeleteConfirmMsg: (count) =>
      $t("billing.deleteConfirm.selected", { count }),
    actionLabel: $t("common.actions.delete"),
  });
const formatDateOnly = (date?: string | Date | null) => {
  return formatDate(date);
};
const formatDateKey = (date?: string | null) => {
  if (!date) return "unknown-date";
  const parsed = new Date(date);
  if (Number.isNaN(parsed.getTime())) return "unknown-date";
  return parsed.toISOString().slice(0, 10).replace(/-/g, "");
};
const getBillKindLabel = (kind?: string | null) => {
  const labels: Record<string, string> = {
    customer: $t("billing.billKinds.customer"),
    upstream: $t("billing.billKinds.upstream"),
  };
  return labels[kind || "customer"] || kind || "-";
};
const getBillCellText = (columnKey: string, bill: Bill) => {
  switch (columnKey) {
    case "tenant_name":
      return bill.tenant_name || bill.party_name || "-";
    case "tenant_code":
      return bill.tenant_code || bill.party_code || "-";
    case "bill_kind":
      return getBillKindLabel(bill.bill_kind);
    case "period_start":
      return formatDateOnly(bill.period_start);
    case "period_end":
      return formatDateOnly(bill.period_end);
    case "total":
      return formatAmount(bill.total);
    case "operations":
      return $t("common.actions.details");
    default:
      return getBillDisplayName(bill);
  }
};
const getBillColumnWeight = (columnKey: string) => {
  switch (columnKey) {
    case "tenant_name":
      return 2.2;
    case "tenant_code":
      return 1.2;
    case "bill_kind":
      return 0.9;
    case "period_start":
    case "period_end":
      return 1;
    case "total":
      return 0.9;
    default:
      return 1;
  }
};
const getBillDisplayName = (bill: Bill | null) => {
  if (!bill) return "-";
  if (bill.bill_kind === "upstream") {
    return (
      bill.party_name || bill.tenant_name || $t("billing.billKinds.upstream")
    );
  }
  return bill.tenant_name || bill.party_name || "-";
};
useAdaptiveTableColumns({
  columns,
  containerRef: pageRef,
  rows: bills,
  baseColumns: billColumns,
  getCellText: ({ columnKey, row }) => getBillCellText(columnKey, row),
  getColumnWeight: ({ columnKey }) => getBillColumnWeight(columnKey),
});
const buildBillDetailCsvRows = (bill: Bill, items: BillLineItem[]) => [
  [$t("billing.labels.billKind"), getBillKindLabel(bill.bill_kind)],
  [$t("billing.labels.tenantName"), bill.tenant_name],
  [$t("billing.labels.tenantCode"), bill.tenant_code],
  [$t("billing.labels.tenantId"), bill.tenant_id],
  [$t("billing.labels.billId"), bill.id],
  [$t("billing.labels.periodStart"), formatDateOnly(bill.period_start)],
  [$t("billing.labels.periodEnd"), formatDateOnly(bill.period_end)],
  [$t("billing.labels.total"), bill.total],
  [$t("billing.labels.totalCalls"), bill.total_calls ?? 0],
  [$t("billing.labels.dataVolume"), formatBytes(bill.total_data_bytes ?? 0)],
  [],
  [
    $t("billing.itemColumns.description"),
    $t("billing.itemColumns.quantity"),
    $t("billing.itemColumns.totalUnits"),
    $t("billing.itemColumns.unitPrice"),
    $t("billing.itemColumns.total"),
  ],
  ...items.map((item) => [
    item.description,
    item.quantity,
    item.total_units,
    item.unit_price,
    item.total,
  ]),
];
const serializeCsv = (rows: unknown[][]) => serializeRawCsv(rows);
const buildBillCsvName = (bill: Bill) =>
  `${bill.party_code || bill.tenant_code || bill.tenant_id}-${formatDateKey(bill.period_start)}-${formatDateKey(bill.period_end)}-${bill.id}.csv`;
const buildBillArchiveName = (prefix: string) =>
  `${prefix}-${Date.now().toString(36)}.tar.gz`;
const exportBillsArchive = async (rows: Bill[], prefix: string) => {
  if (rows.length === 0) {
    ElMessage.warning($t("billing.messages.noExport"));
    return;
  }
  await runWithLoading(exporting, async () => {
    const details = await Promise.all(rows.map((row) => fetchBillById(row.id)));
    const encoder = new TextEncoder();
    const files = details.map(({ bill, items }) => ({
      name: buildBillCsvName(bill),
      content: encoder.encode(
        `${serializeCsv(buildBillDetailCsvRows(bill, items))}\n`,
      ),
    }));
    const archive = await gzipBytes(createTarArchive(files));
    downloadBinary(buildBillArchiveName(prefix), archive, "application/gzip");
  }).catch((error) => {
    logger.error("Failed to export bill archive:", error);
    ElMessage.error($t("billing.messages.exportDetailFailed"));
  });
};
const loadData = async () => {
  await runWithLoading(loading, async () => {
    const res = await fetchBillList({
      ...searchForm,
      page: pagination.page,
      limit: pagination.limit,
      filters: listFilters.value.length ? JSON.stringify(listFilters.value) : undefined,
    });
    bills.value = res.bills;
    pagination.total = res.total;
  }).catch((error) => {
    logger.error("Failed to load bills:", error);
  });
};
const loadTenants = async () => {
  if (generateForm.bill_kind !== "customer") {
    return;
  }
  await runWithLoading(tenantsLoading, async () => {
    const response = await fetchTenantList({ page_size: 1000 });
    tenants.value = response.tenants;
  }).catch((error) => {
    logger.error("Failed to load tenants for bill generation:", error);
    ElMessage.error($t("billing.messages.loadTenantsFailed"));
  });
};
const loadChannels = async () => {
  if (
    generateForm.bill_kind !== "upstream" ||
    generateForm.upstream_scope_type !== "channel"
  ) {
    return;
  }
  await runWithLoading(channelsLoading, async () => {
    const response = await fetchChannels({ page_size: 1000 });
    channels.value = response.channels;
  }).catch((error) => {
    logger.error("Failed to load channels for bill generation:", error);
    ElMessage.error($t("billing.messages.loadChannelsFailed"));
  });
};
const loadSuppliers = async () => {
  if (
    generateForm.bill_kind !== "upstream" ||
    generateForm.upstream_scope_type !== "supplier"
  ) {
    return;
  }
  await runWithLoading(suppliersLoading, async () => {
    const response = await fetchSuppliers({ page_size: 1000 });
    suppliers.value = response.suppliers;
  }).catch((error) => {
    logger.error("Failed to load suppliers for bill generation:", error);
    ElMessage.error($t("billing.messages.loadSuppliersFailed"));
  });
};
const handlePageSizeChange = (size: number) => {
  pagination.limit = size;
  pagination.page = 1;
  loadData();
};
const handleCurrentPageChange = (page: number) => {
  pagination.page = page;
  loadData();
};
const handleSearch = () => {
  pagination.page = 1;
  loadData();
};

const loadBillAdjustments = async (billId: string) => {
  adjustmentsLoading.value = true;
  try {
    const res = await fetchBillAdjustments(billId);
    billAdjustments.value = res.items ?? [];
  } catch (error) {
    logger.error("Failed to load bill adjustments:", error);
    billAdjustments.value = [];
  } finally {
    adjustmentsLoading.value = false;
  }
};

const handleView = async (row: Bill) => {
  detailVisible.value = true;
  currentBill.value = row;
  billItems.value = [];
  billAdjustments.value = [];
  await runWithLoading(detailLoading, async () => {
    const detail = await fetchBillById(row.id);
    currentBill.value = detail.bill;
    billItems.value = detail.items;
    await loadBillAdjustments(detail.bill.id);
  }).catch((error) => {
    logger.error("Failed to load bill detail:", error);
    ElMessage.error($t("billing.messages.loadBillDetailFailed"));
  });
};

const openCreateAdjustment = () => {
  adjustmentForm.adjustment_type = "credit";
  adjustmentForm.amount = "";
  adjustmentForm.reason = "";
  adjustmentDialogVisible.value = true;
};

const handleCreateBillAdjustment = async () => {
  if (!currentBill.value) return;
  if (!adjustmentForm.amount.trim() || !adjustmentForm.reason.trim()) {
    ElMessage.warning($t("billing.adjustments.required"));
    return;
  }
  adjustmentSaving.value = true;
  try {
    await createBillAdjustment(currentBill.value.id, {
      adjustment_type: adjustmentForm.adjustment_type,
      amount: adjustmentForm.amount.trim(),
      reason: adjustmentForm.reason.trim(),
    });
    adjustmentDialogVisible.value = false;
    await loadBillAdjustments(currentBill.value.id);
  } catch (error) {
    logger.error("Failed to create bill adjustment:", error);
  } finally {
    adjustmentSaving.value = false;
  }
};

const handleVoidAdjustment = async (row: BillAdjustment) => {
  try {
    await ElMessageBox.confirm(
      $t("billing.adjustments.voidConfirm"),
      $t("billing.adjustments.void"),
      { type: "warning" },
    );
    await voidBillAdjustment(row.id);
    if (currentBill.value) {
      await loadBillAdjustments(currentBill.value.id);
    }
  } catch (error) {
    if (error !== "cancel") {
      logger.error("Failed to void bill adjustment:", error);
    }
  }
};
const resetGenerateForm = () => {
  generateForm.bill_kind = "customer";
  generateForm.tenant_code = "";
  generateForm.upstream_scope_type = "channel";
  generateForm.upstream_scope_id = "";
  generateForm.period_range = [];
  generateFormRef.value?.resetFields();
};
const handleOpenGenerateDialog = async () => {
  generateDialogVisible.value = true;
  if (generateForm.bill_kind === "customer" && tenants.value.length === 0) {
    await loadTenants();
  }
  if (generateForm.bill_kind === "upstream") {
    if (
      generateForm.upstream_scope_type === "channel" &&
      channels.value.length === 0
    ) {
      await loadChannels();
    }
    if (
      generateForm.upstream_scope_type === "supplier" &&
      suppliers.value.length === 0
    ) {
      await loadSuppliers();
    }
  }
};
const toIsoAtBoundary = (date: string, endOfDay = false) => {
  const suffix = endOfDay ? "23:59:59.999" : "00:00:00.000";
  return new Date(`${date}T${suffix}`).toISOString();
};
const resolveGenerateBillError = (error: unknown) => {
  const message =
    getHttpErrorRawMessage(error) ??
    (error instanceof Error ? error.message : String(error ?? ""));
  const normalized = message.toLowerCase();
  if (normalized.includes("already exists")) {
    return $t("billing.messages.billAlreadyExists");
  }
  if (
    normalized.includes("no upstream request logs") ||
    normalized.includes("no billable upstream request logs")
  ) {
    return $t("billing.messages.noUpstreamLogs");
  }
  if (normalized.includes("period start")) {
    return $t("billing.messages.invalidPeriod");
  }
  return $t("billing.messages.generateFailed");
};
const handleGenerateBill = async () => {
  const valid = await generateFormRef.value?.validate().catch(() => false);
  if (!valid) return;
  if (generateForm.period_range.length !== 2) {
    ElMessage.error($t("billing.validation.periodRangeComplete"));
    return;
  }
  const [periodStart, periodEnd] = generateForm.period_range;
  const payload: GenerateBillRequest = {
    bill_kind: generateForm.bill_kind,
    period_start: toIsoAtBoundary(periodStart),
    period_end: toIsoAtBoundary(periodEnd, true),
  };
  if (generateForm.bill_kind === "customer") {
    payload.tenant_code = generateForm.tenant_code;
  } else {
    payload.upstream_scope_type = generateForm.upstream_scope_type;
    payload.upstream_scope_id = generateForm.upstream_scope_id;
  }
  await runWithLoading(generating, async () => {
    if (generateForm.bill_kind === "upstream") {
      await generateBillAsync(payload);
    } else {
      await generateBill(payload);
    }
    ElMessage.success($t("billing.messages.generateSuccess"));

    generateDialogVisible.value = false;
    resetGenerateForm();
    await loadData();
  }).catch((error) => {
    logger.error("Failed to generate bill:", error);
    ElMessage.error(resolveGenerateBillError(error));
  });
};
const handleDeleteRow = async (row: Bill) => {
  try {
    await ElMessageBox.confirm(
      $t("billing.deleteConfirm.one", { name: getBillDisplayName(row) }),
      $t("billing.deleteConfirm.title"),
      {
        confirmButtonText: $t("common.actions.delete"),
        cancelButtonText: $t("common.cancel"),
        type: "warning",
      },
    );
    await deleteBill(row.id);
    await loadData();
    selectedRows.value = [];
  } catch (error) {
    if (error !== "cancel") {
      logger.error("Failed to delete bill:", error);
    }
  }
};
const handleExportCommand = (command: "page" | "selected") => {
  if (command === "page") {
    void exportBillsArchive([...bills.value], "bills-page");
    return;
  }
  void exportBillsArchive([...selectedRows.value], "bills-selected");
};
const handleExportDetail = () => {
  if (!currentBill.value) return;
  const bill = currentBill.value;
  const summaryRows = buildBillDetailCsvRows(bill, billItems.value);
  downloadCsv(`\uFEFF${serializeCsv(summaryRows)}`, buildBillCsvName(bill));
};
onMounted(() => {
  loadData();
});
watch(generateDialogVisible, (visible) => {
  if (!visible) {
    resetGenerateForm();
  }
});
watch(
  () => generateForm.bill_kind,
  (kind) => {
    if (kind === "upstream") {
      generateForm.tenant_code = "";
      if (!generateForm.upstream_scope_type) {
        generateForm.upstream_scope_type = "channel";
      }
      if (
        generateForm.upstream_scope_type === "channel" &&
        channels.value.length === 0
      ) {
        void loadChannels();
      }
      if (
        generateForm.upstream_scope_type === "supplier" &&
        suppliers.value.length === 0
      ) {
        void loadSuppliers();
      }
      return;
    }
    if (kind === "customer" && tenants.value.length === 0) {
      void loadTenants();
    }
  },
);
watch(
  () => generateForm.upstream_scope_type,
  (type, oldType) => {
    if (type === oldType) return;
    generateForm.upstream_scope_id = "";
    if (generateForm.bill_kind !== "upstream") return;
    if (type === "channel" && channels.value.length === 0) {
      void loadChannels();
    }
    if (type === "supplier" && suppliers.value.length === 0) {
      void loadSuppliers();
    }
  },
);
</script>

<style scoped lang="scss">
.bill-ops-page {
  min-width: 0;
}

.tenant-name-cell {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.tenant-name-text {
  font-weight: 600;
}

.tenant-code-cell {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.tenant-code-text {
  font-size: 12px;
  color: var(--el-text-color-secondary);
}

.bill-detail-layout {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.bill-detail-toolbar {
  display: flex;
  justify-content: flex-end;
}

.bill-detail-overview-card,
.bill-detail-items-card {
  border-radius: 16px;
}

.bill-detail-descriptions {
  margin-top: 2px;
}

.bill-detail-items-table {
  width: 100%;
}

@media (max-width: 768px) {
  .bill-detail-toolbar :deep(.el-button) {
    width: 100%;
  }
}
</style>
