<template>
  <ControlPanelPageShell
    :title="$t('menus.mazbri.callbackList')"
    icon="ri:webhook-line"
  >
    <div class="callback-list-page" style="display: flex; flex-direction: column; flex: 1; min-height: 0;">
    <div class="tab-segmented">
      <ElSegmented
        :model-value="activeTab"
        :options="tabOptions"
        @change="handleTabChange"
      />
    </div>

    <div v-show="activeTab === 'subscriptions'" class="tab-panel" style="display: flex; flex-direction: column; flex: 1; min-height: 0;">
      <ResourceTableShell
        v-model:columns="subscriptionColumns"
        :title="$t('callbacks.tabs.subscriptions')"
        :loading="loading"
        :data="tableData"
        :show-toolbar="false"
        stripe
        border
        @selection-change="handleSelectionChange"
      >
        <template #actions>
          <ListQueryBar
            v-model:q="subscriptionQ"
            v-model:filters="subscriptionFilters"
            :filter-catalog="callbackSubscriptionsCatalog"
            @search="subscriptionSearch.handleSearch"
          />
          <ElButton @click="openImport">
            <template #icon><ArtSvgIcon icon="ri:upload-line" /></template>
            {{ $t("common.actions.import") }}
          </ElButton>
          <TableExportDropdown
            :disabled="tableData.length === 0"
            :items="subscriptionExportItems"
            @command="handleSubscriptionExport"
          />
          <ElButton
            v-roles="['R_ADMIN', 'R_OPERATOR']"
            type="primary"
            @click="createVisible = true"
          >
            <template #icon><ArtSvgIcon icon="ri:add-line" /></template>
            {{ $t("callbacks.actions.addSubscription") }}
          </ElButton>
          <TableDeleteDropdown
            v-roles="['R_ADMIN', 'R_OPERATOR']"
            :total-count="tableData.length"
            :selected-count="selectedRows.length"
            @command="(type) => handleDeleteCommand(type, tableData)"
          />
        </template>

        <template #is_active="{ row }">
          <StatusIndicator :type="row.is_active ? 'success' : 'info'">
            {{
              row.is_active
                ? $t("common.status.enabled")
                : $t("common.status.disabled")
            }}
          </StatusIndicator>
        </template>
        <template #operations="{ row }">
          <TableActionRow>
            <ElButton
              v-roles="['R_ADMIN', 'R_OPERATOR']"
              link
              type="danger"
              @click="handleDelete(row)"
            >
              {{ $t("common.actions.delete") }}
            </ElButton>
          </TableActionRow>
        </template>
      </ResourceTableShell>
    </div>

    <div v-show="activeTab === 'deliveries'" class="tab-panel" style="display: flex; flex-direction: column; flex: 1; min-height: 0;">
      <ResourceTableShell
        v-model:columns="deliveryColumns"
        :title="$t('callbacks.tabs.deliveries')"
        :loading="deliveryLoading"
        :data="deliveryData"
        :show-toolbar="false"
        stripe
        border
      >
        <template #actions>
          <ListQueryBar
            v-model:q="deliveryQ"
            v-model:filters="deliveryFilters"
            :filter-catalog="callbackDeliveriesCatalog"
            @search="deliverySearch.handleSearch"
          />
        </template>

        <template #status="{ row }">
          <StatusIndicator :type="deliveryStatusType(row.status)">
            {{ deliveryStatusLabel(row.status) }}
          </StatusIndicator>
        </template>
        <template #operations="{ row }">
          <TableActionRow>
            <ElButton
              v-roles="['R_ADMIN', 'R_OPERATOR']"
              link
              type="primary"
              :disabled="row.status === 'success'"
              @click="handleRetry(row)"
            >
              {{ $t("callbacks.actions.retry") }}
            </ElButton>
          </TableActionRow>
        </template>
      </ResourceTableShell>
    </div>

    <CreateDialog v-model="createVisible" @success="getData" />
    <ImportDialog
      v-model="importVisible"
      :title="$t('common.actions.import')"
      :columns="subscriptionImportColumns"
      :format-hint="subscriptionImportFormatHint"
      unique-key="unique_key"
      :existing-data="subscriptionImportExistingData"
      :validator="validateSubscriptionImportRow"
      :transformer="transformSubscriptionImportRow"
      :on-import="handleSubscriptionImport"
      :show-success-message="false"
      :show-error-message="false"
      @success="getData"
    />
    </div>
  </ControlPanelPageShell>
</template>

<script setup lang="ts">
import { callbackSubscriptionsCatalog, callbackDeliveriesCatalog } from "@/list-catalog/callbacks";
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import { computed, onMounted, reactive, ref, watch } from "vue";
import { useTabRouteSync } from "@/composables/useTabRouteSync";
import { ElMessageBox, ElMessage } from "element-plus";
import {
  createCallbackSubscription,
  fetchCallbackSubscriptions,
  deleteCallbackSubscription,
  fetchCallbackDeliveries,
  retryCallbackDelivery,
} from "@/api/callback";
import type { CallbackDeliveryLog, CallbackSubscription } from "@/api/callback";
import { $t } from "@/locales";
import CreateDialog from "./modules/create-dialog.vue";
import ImportDialog from "@/components/common/ImportDialog.vue";
import StatusIndicator from "@/components/common/StatusIndicator.vue";
import ListQueryBar from "@/components/common/list-query-bar/index.vue";
import ResourceTableShell from "@/components/common/resource-table-shell/index.vue";
import TableDeleteDropdown from "@/components/table/TableDeleteDropdown.vue";
import TableExportDropdown from "@/components/table/TableExportDropdown.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { useListQuery } from "@/composables/useListQuery";
import {
  exportResourceCsv,
  runResourceCsvImport,
} from "@/composables/useResourceCsvImportExport";
import { useTableSelection } from "@/composables/useTableSelection";
import type { ColumnConfig } from "@/composables/useResourceCsvImportExport";
import type { ColumnOption } from "@/types/component";
import { getUserFacingErrorMessage } from "@/utils/http/error";
import { logger } from "@/utils/sys";
defineOptions({ name: "CallbackList" });
type CallbackTab = "subscriptions" | "deliveries";
type ExportCommand = "page" | "selected";
interface SubscriptionImportRow extends Record<string, unknown> {
  unique_key: string;
  tenant_code: string;
  product_code: string;
  callback_url: string;
  secret_key: string;
}
interface SubscriptionExportRow extends Record<string, unknown> {
  tenant_code: string;
  tenant_id: string;
  product_code: string;
  callback_url: string;
  status: string;
}
const loading = ref(false);
const deliveryLoading = ref(false);
const createVisible = ref(false);
const importVisible = ref(false);
const { activeTab, handleTabChange } = useTabRouteSync<CallbackTab>({
  defaultValue: "subscriptions",
  allowedValues: ["subscriptions", "deliveries"],
});
const tabOptions = [
  { label: $t("callbacks.tabs.subscriptions"), value: "subscriptions" },
  { label: $t("callbacks.tabs.deliveries"), value: "deliveries" },
];
const tableData = ref<CallbackSubscription[]>([]);
const deliveryData = ref<CallbackDeliveryLog[]>([]);
const subscriptionSearch = useListQuery({
  catalog: callbackSubscriptionsCatalog,
  searchOnQChange: false,
  encodeMode: "json",
  onChange: () => {
    void getData();
  },
});
const deliverySearch = useListQuery({
  catalog: callbackDeliveriesCatalog,
  searchOnQChange: false,
  encodeMode: "json",
  onChange: () => {
    void getDeliveries();
  },
});
const subscriptionQ = subscriptionSearch.q;
const deliveryQ = deliverySearch.q;
const subscriptionFilters = subscriptionSearch.filters;
const deliveryFilters = deliverySearch.filters;
const subscriptionColumns = ref<ColumnOption<CallbackSubscription>[]>([
  { type: "selection", width: 55 },
  {
    prop: "tenant_code",
    label: $t("callbacks.columns.tenantCode"),
    width: 120,
  },
  {
    prop: "tenant_id",
    label: $t("callbacks.columns.tenantAccount"),
    width: 150,
    showOverflowTooltip: true,
  },
  {
    prop: "product_code",
    label: $t("callbacks.columns.productCode"),
    width: 150,
    showOverflowTooltip: true,
  },
  {
    prop: "callback_url",
    label: $t("callbacks.columns.callbackUrl"),
    minWidth: 150,
    showOverflowTooltip: true,
  },
  {
    prop: "is_active",
    label: $t("common.fields.status"),
    width: 100,
    useSlot: true,
  },
  {
    prop: "operations",
    label: $t("common.fields.operations"),
    width: 120,
    fixed: "right",
    useSlot: true,
  },
]);
const deliveryColumns = ref<ColumnOption<CallbackDeliveryLog>[]>([
  {
    prop: "event_id",
    label: $t("callbacks.columns.eventId"),
    width: 180,
    showOverflowTooltip: true,
  },
  {
    prop: "callback_url",
    label: $t("callbacks.columns.callbackUrl"),
    minWidth: 140,
    showOverflowTooltip: true,
  },
  {
    prop: "status",
    label: $t("common.fields.status"),
    width: 110,
    useSlot: true,
  },
  {
    prop: "retry_count",
    label: $t("callbacks.columns.retryCount"),
    width: 100,
  },
  {
    prop: "last_response_code",
    label: $t("callbacks.columns.responseCode"),
    width: 100,
  },
  {
    prop: "last_error",
    label: $t("callbacks.columns.failureReason"),
    minWidth: 120,
    showOverflowTooltip: true,
  },
  {
    prop: "operations",
    label: $t("common.fields.operations"),
    width: 120,
    fixed: "right",
    useSlot: true,
  },
]);
const subscriptionImportColumns: ColumnConfig[] = [
  {
    key: "tenant_code",
    label: $t("callbacks.columns.tenantCode"),
    required: true,
  },
  {
    key: "product_code",
    label: $t("callbacks.columns.productCode"),
    required: true,
  },
  {
    key: "callback_url",
    label: $t("callbacks.columns.callbackUrl"),
    required: true,
  },
  { key: "secret_key", label: $t("callbacks.form.secretKey"), required: true },
];
const subscriptionImportExistingData = computed(() =>
  tableData.value.map((row) => ({
    unique_key: `${row.tenant_code}/${row.product_code}`,
  })),
);
const subscriptionImportFormatHint = computed(() =>
  [
    $t("feedback.import.csvFormatHint"),
    $t("feedback.import.requiredColumns", {
      columns: subscriptionImportColumns
        .filter((column) => column.required)
        .map((column) => column.key)
        .join(", "),
    }),
  ].join("\n"),
);
const {
  selectedRows,
  handleSelectionChange,
  handleDeleteCommand,
  clearSelection,
} = useTableSelection<CallbackSubscription>({
  deleteApi: async (row) => {
    await deleteCallbackSubscription(row.id);
  },
  getItemName: (row) => `${row.tenant_code}/${row.product_code}`,
  onDeleteSuccess: () => getData(),
});
const subscriptionExportItems = computed(() => [
  {
    command: "page",
    label: $t("common.actions.exportPage"),
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
watch(subscriptionQ, () => {
  clearSelection();
});
const getData = async () => {
  await runWithLoading(loading, async () => {
    const res = await fetchCallbackSubscriptions({
      ...(subscriptionSearch.toRequestParams() as {
        q?: string;
        filters?: string;
      }),
      page: 1,
      page_size: 500,
    });
    tableData.value = res.data ?? [];
    clearSelection();
  }).catch((error: unknown) => {
    ElMessage.error(
      getUserFacingErrorMessage(
        error,
        $t("callbacks.messages.fetchListFailed"),
      ),
    );
  });
};
const getDeliveries = async () => {
  await runWithLoading(deliveryLoading, async () => {
    const res = await fetchCallbackDeliveries({
      ...(deliverySearch.toRequestParams() as {
        q?: string;
        filters?: string;
      }),
      page: 1,
      page_size: 500,
    });
    deliveryData.value = res.data ?? [];
  }).catch((error: unknown) => {
    ElMessage.error(
      getUserFacingErrorMessage(
        error,
        $t("callbacks.messages.fetchDeliveriesFailed"),
      ),
    );
  });
};
const parseCallbackImport = (value: string) => value.trim();
const validateSubscriptionImportRow = (row: Record<string, string>) => {
  if (!row.tenant_code?.trim()) {
    return {
      valid: false,
      reason: $t("callbacks.validation.tenantCodeRequired"),
    };
  }
  if (!row.product_code?.trim()) {
    return {
      valid: false,
      reason: $t("callbacks.validation.productCodeRequired"),
    };
  }
  if (!row.callback_url?.trim()) {
    return {
      valid: false,
      reason: $t("callbacks.validation.callbackUrlRequired"),
    };
  }
  if (!row.secret_key?.trim()) {
    return {
      valid: false,
      reason: $t("callbacks.validation.secretKeyRequired"),
    };
  }
  return { valid: true };
};
const transformSubscriptionImportRow = (
  row: Record<string, string>,
): SubscriptionImportRow => ({
  unique_key: `${parseCallbackImport(row.tenant_code)}/${parseCallbackImport(row.product_code)}`,
  tenant_code: parseCallbackImport(row.tenant_code),
  product_code: parseCallbackImport(row.product_code),
  callback_url: parseCallbackImport(row.callback_url),
  secret_key: parseCallbackImport(row.secret_key),
});
const openImport = () => {
  importVisible.value = true;
};
const handleSubscriptionExport = (command: string) => {
  const exportCommand = command as ExportCommand;
  const rows =
    exportCommand === "selected"
      ? selectedRows.value
      : tableData.value;
  const exportRows: SubscriptionExportRow[] = rows.map((row) => ({
    tenant_code: row.tenant_code,
    tenant_id: row.tenant_id,
    product_code: row.product_code,
    callback_url: row.callback_url,
    status: row.is_active
      ? $t("common.status.enabled")
      : $t("common.status.disabled"),
  }));
  exportResourceCsv(
    exportRows,
    [
      { key: "tenant_code", label: $t("callbacks.columns.tenantCode") },
      { key: "tenant_id", label: $t("callbacks.columns.tenantAccount") },
      { key: "product_code", label: $t("callbacks.columns.productCode") },
      { key: "callback_url", label: $t("callbacks.columns.callbackUrl") },
      { key: "status", label: $t("common.fields.status") },
    ],
    `callback-subscriptions-${exportCommand}-${Date.now().toString(36)}.csv`,
  );
};
const handleSubscriptionImport = async (items: Record<string, unknown>[]) => {
  await runResourceCsvImport<SubscriptionImportRow>(
    items,
    async (row) => {
      await createCallbackSubscription(
        {
          tenant_code: row.tenant_code,
          product_code: row.product_code,
          callback_url: row.callback_url,
          secret_key: row.secret_key,
        },
        { showSuccessMessage: false },
      );
    },
    {
      onError: (error) => {
        logger.error("Failed to import callback subscription:", error);
      },
    },
  );
};
const handleDelete = async (row: CallbackSubscription) => {
  try {
    await ElMessageBox.confirm(
      $t("callbacks.messages.deleteConfirm"),
      $t("common.tips"),
      {
        type: "warning",
      },
    );
    await deleteCallbackSubscription(row.id);
    ElMessage.success($t("common.feedback.deleteSuccess"));
    getData();
  } catch (err) {
    logger.debug("Delete callback subscription canceled or failed", err);
  }
};
const handleRetry = async (row: CallbackDeliveryLog) => {
  try {
    await retryCallbackDelivery(row.id);
    ElMessage.success($t("callbacks.messages.retrySuccess"));
    getDeliveries();
  } catch (error: unknown) {
    ElMessage.error(
      getUserFacingErrorMessage(error, $t("callbacks.messages.retryFailed")),
    );
  }
};
const deliveryStatusType = (status: string) => {
  if (status === "success") return "success";
  if (status === "failed" || status === "abandoned") return "danger";
  if (status === "retrying") return "warning";
  return "info";
};
const deliveryStatusLabel = (status: string) => {
  if (status === "success") return $t("callbacks.status.success");
  if (status === "failed") return $t("callbacks.status.failed");
  if (status === "abandoned") return $t("callbacks.status.abandoned");
  if (status === "retrying") return $t("callbacks.status.retrying");
  return status || "-";
};
onMounted(() => {
  getData();
  getDeliveries();
});
</script>

<style scoped lang="scss">
.callback-list-page {
  min-width: 0;
  display: flex;
  flex-direction: column;
  flex: 1;
  height: 100%;

  .tab-segmented {
    margin-bottom: 16px;
    flex-shrink: 0;

    .el-segmented {
      height: 40px;
      padding: 4px;
    }
  }

  .tab-panel {
    min-width: 0;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }

  .tab-panel :deep(.surface-panel-table) {
    margin-top: 0 !important;
  }
}
</style>
