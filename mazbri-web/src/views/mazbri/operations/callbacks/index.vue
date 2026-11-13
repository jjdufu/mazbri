<template>
  <ControlPanelPageShell
    :title="$t('menus.mazbri.callbackList')"
    icon="ri:webhook-line"
  >
    <div class="callback-list-page">
      <div class="page-header-tabs-bar">
        <ElTabs
          :model-value="activeTab"
          class="page-header-tabs"
          @tab-change="handleTabChange"
        >
          <ElTabPane
            v-for="item in tabOptions"
            :key="item.value"
            :label="item.label"
            :name="item.value"
          />
        </ElTabs>
        <div ref="headerActionsEl" class="page-header-tabs-actions" />
      </div>

      <div v-show="activeTab === 'subscriptions'" class="tab-panel">
        <ResourceTableShell
          v-model:columns="subscriptionColumns"
          :title="$t('callbacks.tabs.subscriptions')"
          :loading="loading"
          :data="tableData"
          :pagination="subscriptionPagination"
          stripe
          @refresh="getData"
          @selection-change="handleSelectionChange"
          @pagination:size-change="handleSubscriptionSizeChange"
          @pagination:current-change="handleSubscriptionCurrentChange"
        >
          <template #actions>
            <ListQueryBar
              v-model:q="subscriptionQ"
              v-model:filters="subscriptionFilters"
              :catalog="callbackSubscriptionsCatalog"
              @search="subscriptionSearch.handleSearch"
            />
            <ElButton
              v-if="canWriteBusiness"
              v-roles="ROLES_BIZ"
              @click="openImport"
            >
              <template #icon><ArtSvgIcon icon="ri:upload-line" /></template>
              {{ $t("common.actions.import") }}
            </ElButton>
            <TableExportDropdown
              :disabled="tableData.length === 0"
              :items="subscriptionExportItems"
              @command="handleSubscriptionExport"
            />
            <ElButton
              v-if="canWriteBusiness"
              v-roles="ROLES_BIZ"
              type="primary"
              @click="createVisible = true"
            >
              <template #icon><ArtSvgIcon icon="ri:add-line" /></template>
              {{ $t("callbacks.actions.addSubscription") }}
            </ElButton>
            <TableDeleteDropdown
              v-if="canWriteBusiness"
              v-roles="ROLES_BIZ"
              :total-count="tableData.length"
              :selected-count="selectedRows.length"
              @command="(type) => handleDeleteCommand(type, tableData)"
            />
          </template>

          <template #tenant_code="{ row }">
            <CopyableText
              :value="row.tenant_code"
              :label="$t('callbacks.columns.tenantCode')"
            />
          </template>
          <template #tenant_id="{ row }">
            <CopyableText
              :value="row.tenant_id"
              :label="$t('callbacks.columns.tenantAccount')"
              compact
            />
          </template>
          <template #product_code="{ row }">
            <CopyableText
              :value="row.product_code"
              :label="$t('callbacks.columns.productCode')"
            />
          </template>
          <template #callback_url="{ row }">
            <CopyableText
              :value="row.callback_url"
              :label="$t('callbacks.columns.callbackUrl')"
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
                v-if="canWriteBusiness"
                v-roles="ROLES_BIZ"
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

      <div v-show="activeTab === 'deliveries'" class="tab-panel">
        <ResourceTableShell
          v-model:columns="deliveryColumns"
          :title="$t('callbacks.tabs.deliveries')"
          :loading="deliveryLoading"
          :data="deliveryData"
          :pagination="deliveryPagination"
          stripe
          @refresh="getDeliveries"
          @pagination:size-change="handleDeliverySizeChange"
          @pagination:current-change="handleDeliveryCurrentChange"
        >
          <template #actions>
            <ListQueryBar
              v-model:q="deliveryQ"
              v-model:filters="deliveryFilters"
              :catalog="callbackDeliveriesCatalog"
              @search="deliverySearch.handleSearch"
            />
          </template>

          <template #event_id="{ row }">
            <CopyableText
              :value="row.event_id"
              :label="$t('callbacks.columns.eventId')"
              compact
            />
          </template>
          <template #callback_url="{ row }">
            <CopyableText
              :value="row.callback_url"
              :label="$t('callbacks.columns.callbackUrl')"
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
                v-if="canWriteBusiness"
                v-roles="ROLES_BIZ"
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
import { computed, provide, reactive, ref, watch } from "vue";
import { ElMessage, ElMessageBox } from "element-plus";
import { useI18n } from "vue-i18n";
import {
  createCallbackSubscription,
  deleteCallbackSubscription,
  fetchCallbackDeliveries,
  fetchCallbackSubscriptions,
  retryCallbackDelivery,
  type CallbackDeliveryLog,
  type CallbackSubscription,
} from "@/api/callback";
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import CopyableText from "@/components/common/copyable-text/index.vue";
import ImportDialog from "@/components/common/ImportDialog.vue";
import ListQueryBar from "@/components/common/list-query-bar/index.vue";
import ResourceTableShell from "@/components/common/resource-table-shell/index.vue";
import StatusIndicator from "@/components/common/StatusIndicator.vue";
import TableDeleteDropdown from "@/components/table/TableDeleteDropdown.vue";
import TableExportDropdown from "@/components/table/TableExportDropdown.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { useListQuery } from "@/composables/useListQuery";
import {
  exportResourceCsv,
  runResourceCsvImport,
  type ColumnConfig,
} from "@/composables/useResourceCsvImportExport";
import { useTabRouteSync } from "@/composables/useTabRouteSync";
import { useTableSelection } from "@/composables/useTableSelection";
import {
  callbackDeliveriesCatalog,
  callbackSubscriptionsCatalog,
} from "@/list-catalog/callbacks";
import { $t } from "@/locales";
import { useUserStore } from "@/store/modules/user";
import type { ColumnOption } from "@/types/component";
import { getUserFacingErrorMessage } from "@/utils/http/error";
import { ROLES_BIZ } from "@/utils/ops/roles";
import { logger } from "@/utils/sys";
import CreateDialog from "./modules/create-dialog.vue";

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

const { locale } = useI18n();
const userStore = useUserStore();
const canWriteBusiness = computed(() => userStore.canWriteBusiness);

const headerActionsEl = ref<HTMLElement | null>(null);
provide("resourceTabHeaderActionsEl", headerActionsEl);

const loading = ref(false);
const deliveryLoading = ref(false);
const createVisible = ref(false);
const importVisible = ref(false);

const { activeTab, handleTabChange } = useTabRouteSync<CallbackTab>({
  defaultValue: "subscriptions",
  allowedValues: ["subscriptions", "deliveries"],
});

const tabOptions = computed(() => {
  void locale.value;
  return [
    { label: $t("callbacks.tabs.subscriptions"), value: "subscriptions" },
    { label: $t("callbacks.tabs.deliveries"), value: "deliveries" },
  ];
});

const tableData = ref<CallbackSubscription[]>([]);
const deliveryData = ref<CallbackDeliveryLog[]>([]);
const subscriptionPagination = reactive({
  current: 1,
  size: 20,
  total: 0,
});
const deliveryPagination = reactive({
  current: 1,
  size: 20,
  total: 0,
});

const subscriptionSearch = useListQuery({
  catalog: callbackSubscriptionsCatalog,
  searchOnQChange: false,
  encodeMode: "json",
  onChange: () => {
    subscriptionPagination.current = 1;
    void getData();
  },
});

const deliverySearch = useListQuery({
  catalog: callbackDeliveriesCatalog,
  searchOnQChange: false,
  encodeMode: "json",
  onChange: () => {
    deliveryPagination.current = 1;
    void getDeliveries();
  },
});

const subscriptionQ = subscriptionSearch.q;
const deliveryQ = deliverySearch.q;
const subscriptionFilters = subscriptionSearch.filters;
const deliveryFilters = deliverySearch.filters;

const buildSubscriptionColumns = (): ColumnOption<CallbackSubscription>[] => {
  const cols: ColumnOption<CallbackSubscription>[] = [];
  if (canWriteBusiness.value) {
    cols.push({ type: "selection", width: 55 });
  }
  cols.push(
    {
      prop: "tenant_code",
      useSlot: true,
      label: $t("callbacks.columns.tenantCode"),
      width: 128,
    },
    {
      prop: "tenant_id",
      useSlot: true,
      label: $t("callbacks.columns.tenantAccount"),
      width: 150,
    },
    {
      prop: "product_code",
      useSlot: true,
      label: $t("callbacks.columns.productCode"),
      width: 150,
    },
    {
      prop: "callback_url",
      useSlot: true,
      label: $t("callbacks.columns.callbackUrl"),
      minWidth: 160,
    },
    {
      prop: "is_active",
      label: $t("common.fields.status"),
      width: 104,
      useSlot: true,
    },
  );
  if (canWriteBusiness.value) {
    cols.push({
      prop: "operations",
      label: $t("common.fields.operations"),
      width: 110,
      fixed: "right",
      useSlot: true,
    });
  }
  return cols;
};

const buildDeliveryColumns = (): ColumnOption<CallbackDeliveryLog>[] => {
  const cols: ColumnOption<CallbackDeliveryLog>[] = [
    {
      prop: "event_id",
      useSlot: true,
      label: $t("callbacks.columns.eventId"),
      width: 180,
    },
    {
      prop: "callback_url",
      useSlot: true,
      label: $t("callbacks.columns.callbackUrl"),
      minWidth: 150,
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
      width: 104,
    },
    {
      prop: "last_response_code",
      label: $t("callbacks.columns.responseCode"),
      width: 112,
    },
    {
      prop: "last_error",
      label: $t("callbacks.columns.failureReason"),
      minWidth: 130,
      showOverflowTooltip: true,
    },
  ];
  if (canWriteBusiness.value) {
    cols.push({
      prop: "operations",
      label: $t("common.fields.operations"),
      width: 110,
      fixed: "right",
      useSlot: true,
    });
  }
  return cols;
};

const subscriptionColumns = ref<ColumnOption<CallbackSubscription>[]>(
  buildSubscriptionColumns(),
);
const deliveryColumns = ref<ColumnOption<CallbackDeliveryLog>[]>(
  buildDeliveryColumns(),
);

watch([locale, canWriteBusiness], () => {
  subscriptionColumns.value = buildSubscriptionColumns();
  deliveryColumns.value = buildDeliveryColumns();
});

const subscriptionImportColumns = computed<ColumnConfig[]>(() => {
  void locale.value;
  return [
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
    {
      key: "secret_key",
      label: $t("callbacks.fields.secret"),
      required: true,
    },
  ];
});

const subscriptionImportExistingData = computed(() =>
  tableData.value.map((row) => ({
    unique_key: `${row.tenant_code}/${row.product_code}`,
  })),
);

const subscriptionImportFormatHint = computed(() =>
  [
    $t("feedback.import.csvFormatHint"),
    $t("feedback.import.requiredColumns", {
      columns: subscriptionImportColumns.value
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
      page: subscriptionPagination.current,
      page_size: subscriptionPagination.size,
    });
    tableData.value = res.data ?? [];
    subscriptionPagination.total = res.total ?? 0;
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
      page: deliveryPagination.current,
      page_size: deliveryPagination.size,
    });
    deliveryData.value = res.data ?? [];
    deliveryPagination.total = res.total ?? 0;
  }).catch((error: unknown) => {
    ElMessage.error(
      getUserFacingErrorMessage(
        error,
        $t("callbacks.messages.fetchDeliveriesFailed"),
      ),
    );
  });
};

const loadedTabs = reactive<Record<CallbackTab, boolean>>({
  subscriptions: false,
  deliveries: false,
});

watch(
  activeTab,
  (tab) => {
    if (!loadedTabs[tab]) {
      loadedTabs[tab] = true;
      if (tab === "subscriptions") {
        void getData();
      } else {
        void getDeliveries();
      }
    }
  },
  { immediate: true },
);

const handleSubscriptionSizeChange = (size: number) => {
  subscriptionPagination.size = size;
  subscriptionPagination.current = 1;
  void getData();
};

const handleSubscriptionCurrentChange = (page: number) => {
  subscriptionPagination.current = page;
  void getData();
};

const handleDeliverySizeChange = (size: number) => {
  deliveryPagination.size = size;
  deliveryPagination.current = 1;
  void getDeliveries();
};

const handleDeliveryCurrentChange = (page: number) => {
  deliveryPagination.current = page;
  void getDeliveries();
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
    exportCommand === "selected" ? selectedRows.value : tableData.value;
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
    void getData();
  } catch (err) {
    logger.debug("Delete callback subscription canceled or failed", err);
  }
};

const handleRetry = async (row: CallbackDeliveryLog) => {
  try {
    await retryCallbackDelivery(row.id);
    ElMessage.success($t("callbacks.messages.retrySuccess"));
    void getDeliveries();
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
  return status || "—";
};
</script>

<style scoped lang="scss">
.callback-list-page {
  min-width: 0;
  display: flex;
  flex-direction: column;
  flex: 1;
  height: 100%;

  .page-header-tabs-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px 16px;
    margin-bottom: 12px;
    flex-shrink: 0;
  }

  .page-header-tabs-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-left: auto;
  }

  .page-header-tabs {
    margin-bottom: 0;
    flex-shrink: 0;

    :deep(.el-tabs__header) {
      margin: 0 !important;
    }

    :deep(.el-tabs__content) {
      display: none !important;
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
