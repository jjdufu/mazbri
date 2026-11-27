<template>
  <div ref="pageRef" class="channel-page">
    <ResourceTableShell
      v-model:columns="columns"
      :title="$t('channels.page.title')"
      :loading="loading"
      :data="tableData"
      :empty-text="emptyDescription"
      :pagination="artPagination"
      :fit="true"
      table-layout="auto"
      @refresh="loadData"
      @selection-change="handleSelectionChange"
      @pagination:size-change="handleSizeChange"
      @pagination:current-change="handleCurrentChange"
    >
      <template #actions>
        <ResourceTableActionBar
          v-model="searchParams.q"
          v-model:filters="listFilters"
          :filter-catalog="channelsCatalog"
          :show-import="canWriteBusiness"
          :import-label="$t('common.actions.import')"
          :import-roles="[...ROLES_BIZ]"
          :import-disabled="!canCreateChannel"
          :import-disabled-reason="$t('channels.messages.createSupplierFirst')"
          :show-export="true"
          :export-items="exportItems"
          :export-disabled="tableData.length === 0"
          :show-primary="canWriteBusiness"
          :primary-label="$t('channels.actions.add')"
          :primary-roles="[...ROLES_BIZ]"
          :primary-disabled="!canCreateChannel"
          :primary-disabled-reason="$t('channels.messages.createSupplierFirst')"
          :show-delete="canWriteBusiness"
          :delete-total-count="tableData.length"
          :delete-selected-count="selectedRows.length"
          :delete-roles="[...ROLES_BIZ]"
          @search="handleSearch"
          @import="openImport"
          @export="handleExportCommand"
          @primary="openCreate"
          @delete="(type) => handleDeleteCommand(type, tableData)"
        />
      </template>

      <template #channel_code="{ row }">
        <CopyableText
          :value="row.channel_code"
          :label="$t('channels.fields.channelCode')"
        />
      </template>
      <template #supplier="{ row }">
        {{ row.supplier_name }} ({{ row.supplier_code }})
      </template>
      <template #path_prefix="{ row }">
        {{ row.path_prefix }}
      </template>
      <template #cost_price="{ row }">
        {{ formatCostPrice(row.cost_price) }}
      </template>
      <template #qps_limit="{ row }">
        <ElTag
          v-if="row.qps_limit === 0"
          type="info"
          effect="plain"
          size="small"
        >
          {{ $t("channels.fields.qpsUnlimited") }}
        </ElTag>
        <span v-else>{{ row.qps_limit }}</span>
      </template>
      <template #status="{ row }">
        <StatusIndicator :type="row.status === 'active' ? 'success' : 'info'">
          {{ $t(`channels.status.${row.status}`) }}
        </StatusIndicator>
      </template>
      <template #operations="{ row }">
        <TableActionRow>
          <ElButton link type="primary" @click="handleView(row)">{{
            $t("common.actions.details")
          }}</ElButton>
          <ElButton
            v-if="canWriteBusiness"
            v-roles="ROLES_BIZ"
            link
            type="primary"
            @click="openEdit(row)"
          >{{
            $t("channels.actions.edit")
          }}</ElButton>
          <ElButton
            v-if="canWriteBusiness"
            v-roles="ROLES_BIZ"
            link
            type="danger"
            @click="handleDelete(row)"
          >{{
            $t("common.actions.delete")
          }}</ElButton>
        </TableActionRow>
      </template>
    </ResourceTableShell>

    <ChannelFormDialog
      v-model="formVisible"
      :channel="editingChannel"
      @success="handleFormSuccess"
    />

    <ChannelDetailDrawer
      ref="detailDrawerRef"
      v-model="detailVisible"
      :channel-id="activeChannelId"
      @edit="openEdit"
    />

    <ImportDialog
      v-model="importVisible"
      :title="$t('common.actions.import')"
      :columns="channelImportColumns"
      :format-hint="channelImportFormatHint"
      unique-key="unique_key"
      :existing-data="channelImportExistingData"
      :validator="validateChannelImportRow"
      :transformer="transformChannelImportRow"
      :on-import="handleChannelImport"
      :show-success-message="false"
      :show-error-message="false"
      @success="loadData"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from "vue";
import { ElMessage, ElMessageBox } from "element-plus";
import { useI18n } from "vue-i18n";
import {
  createChannel,
  deleteChannel,
  fetchChannels,
  updateChannel,
  type Channel,
} from "@/api/route-management";
import { fetchSuppliers, type Supplier } from "@/api/supplier";
import CopyableText from "@/components/common/copyable-text/index.vue";
import ImportDialog from "@/components/common/ImportDialog.vue";
import ResourceTableActionBar from "@/components/common/resource-table-action-bar/index.vue";
import ResourceTableShell from "@/components/common/resource-table-shell/index.vue";
import StatusIndicator from "@/components/common/StatusIndicator.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { useAdaptiveTableColumns } from "@/composables/useAdaptiveTableColumns";
import {
  exportResourceCsv,
  runResourceCsvImport,
  type ColumnConfig,
} from "@/composables/useResourceCsvImportExport";
import { useTableSelection } from "@/composables/useTableSelection";
import { channelsCatalog } from "@/list-catalog/common-resources";
import { $t } from "@/locales";
import { useUserStore } from "@/store/modules/user";
import type { ColumnOption, PaginationConfig } from "@/types/component";
import type { ListFilter } from "@/types/list-query";
import { ROLES_BIZ } from "@/utils/ops/roles";
import {
  formatCostPrice,
  isValidCostPrice,
  normalizeCostPriceInput,
} from "./costPrice";
import ChannelDetailDrawer from "./modules/ChannelDetailDrawer.vue";
import ChannelFormDialog from "./modules/ChannelFormDialog.vue";

defineOptions({ name: "UpstreamChannelList" });

const props = defineProps<{
  activeTab?: string;
}>();

const { locale } = useI18n();
const userStore = useUserStore();
const canWriteBusiness = computed(() => userStore.canWriteBusiness);

type ExportCommand = "page" | "selected";

interface ChannelImportRow extends Record<string, unknown> {
  unique_key: string;
  supplier_code: string;
  channel_code: string;
  name: string;
  path_prefix: string;
  cost_price: string;
  qps_limit: number;
  status: string;
}

interface ChannelExportRow extends Record<string, unknown> {
  supplier_code: string;
  channel_code: string;
  name: string;
  path_prefix: string;
  cost_price: string;
  qps_limit: number;
  status: string;
}

const allowedStatuses = new Set(["active", "inactive", "suspended"]);
const pageRef = ref<HTMLElement | null>(null);
const loading = ref(false);
const formVisible = ref(false);
const editingChannel = ref<Channel | null>(null);
const detailVisible = ref(false);
const activeChannelId = ref("");
const detailDrawerRef = ref<InstanceType<typeof ChannelDetailDrawer> | null>(
  null,
);
const importVisible = ref(false);
const hasActiveSupplier = ref(false);
const tableData = ref<Channel[]>([]);
const searchParams = reactive({ q: "" });
const listFilters = ref<ListFilter[]>([]);
const pagination = reactive({
  current: 1,
  size: 20,
  total: 0,
});
const columns = ref<ColumnOption<Channel>[]>([]);

const channelColumns = computed<ColumnOption<Channel>[]>(() => {
  void locale.value;
  const cols: ColumnOption<Channel>[] = [];
  if (canWriteBusiness.value) {
    cols.push({ type: "selection", width: 55 });
  }
  cols.push(
    {
      prop: "channel_code",
      useSlot: true,
      label: $t("channels.fields.channelCode"),
      minWidth: 112,
    },
    {
      prop: "name",
      label: $t("channels.fields.name"),
      minWidth: 120,
      maxWidth: 280,
      showOverflowTooltip: true,
    },
    {
      prop: "supplier",
      label: $t("channels.fields.supplier"),
      minWidth: 132,
      maxWidth: 280,
      showOverflowTooltip: true,
      useSlot: true,
    },
    {
      prop: "path_prefix",
      label: $t("channels.fields.pathPrefix"),
      minWidth: 110,
      showOverflowTooltip: true,
      useSlot: true,
    },
    {
      prop: "cost_price",
      label: $t("channels.fields.costPrice"),
      minWidth: 100,
      useSlot: true,
    },
    {
      prop: "qps_limit",
      label: $t("channels.fields.qpsLimit"),
      minWidth: 116,
      useSlot: true,
    },
    {
      prop: "status",
      label: $t("common.fields.status"),
      width: 104,
      useSlot: true,
    },
    {
      prop: "operations",
      label: $t("common.fields.operations"),
      width: canWriteBusiness.value ? 176 : 96,
      fixed: "right",
      useSlot: true,
    },
  );
  return cols;
});

const getChannelCellText = (columnKey: string, row: Channel) => {
  switch (columnKey) {
    case "channel_code":
      return row.channel_code || "—";
    case "name":
      return row.name || "—";
    case "supplier":
      return `${row.supplier_name || "—"} (${row.supplier_code || "—"})`;
    case "path_prefix":
      return row.path_prefix || "—";
    case "cost_price":
      return formatCostPrice(row.cost_price);
    case "qps_limit":
      return row.qps_limit === 0
        ? $t("channels.fields.qpsUnlimited")
        : String(row.qps_limit ?? "—");
    case "status":
      return $t(`channels.status.${row.status}`);
    case "operations":
      return canWriteBusiness.value
        ? `${$t("common.actions.details")} ${$t("channels.actions.edit")} ${$t("common.actions.delete")}`
        : $t("common.actions.details");
    default:
      return row.name || row.channel_code || "—";
  }
};

const getChannelColumnWeight = (columnKey: string) => {
  switch (columnKey) {
    case "channel_code":
      return 1.1;
    case "name":
      return 1.4;
    case "supplier":
      return 1.8;
    case "path_prefix":
      return 1.5;
    case "cost_price":
      return 0.9;
    case "qps_limit":
      return 0.9;
    case "status":
      return 0.8;
    default:
      return 1;
  }
};

useAdaptiveTableColumns({
  columns,
  containerRef: pageRef,
  rows: tableData,
  baseColumns: channelColumns,
  getCellText: ({ columnKey, row }) => getChannelCellText(columnKey, row),
  getColumnWeight: ({ columnKey }) => getChannelColumnWeight(columnKey),
});

const channelImportColumns = computed<ColumnConfig[]>(() => {
  void locale.value;
  return [
    {
      key: "supplier_code",
      label: $t("channels.fields.supplier"),
      required: true,
    },
    {
      key: "channel_code",
      label: $t("channels.fields.channelCode"),
      required: true,
    },
    { key: "name", label: $t("channels.fields.name"), required: true },
    {
      key: "path_prefix",
      label: $t("channels.fields.pathPrefix"),
      required: true,
    },
    {
      key: "cost_price",
      label: $t("channels.fields.costPrice"),
      required: false,
    },
    {
      key: "qps_limit",
      label: $t("channels.fields.qpsLimit"),
      required: false,
    },
    { key: "status", label: $t("common.fields.status"), required: false },
  ];
});

const channelExportColumns = computed<
  { key: keyof ChannelExportRow; label: string }[]
>(() => {
  void locale.value;
  return [
    { key: "supplier_code", label: $t("channels.fields.supplier") },
    { key: "channel_code", label: $t("channels.fields.channelCode") },
    { key: "name", label: $t("channels.fields.name") },
    { key: "path_prefix", label: $t("channels.fields.pathPrefix") },
    { key: "cost_price", label: $t("channels.fields.costPrice") },
    { key: "qps_limit", label: $t("channels.fields.qpsLimit") },
    { key: "status", label: $t("common.fields.status") },
  ];
});

const channelImportExistingData = computed(() =>
  tableData.value.map((row) => ({
    unique_key: `${row.supplier_code}:${row.channel_code}`,
  })),
);

const channelImportFormatHint = computed(() =>
  [
    $t("feedback.import.csvFormatHint"),
    $t("feedback.import.requiredColumns", {
      columns: channelImportColumns.value
        .filter((column) => column.required)
        .map((column) => column.key)
        .join(", "),
    }),
  ].join("\n"),
);

const artPagination = computed<PaginationConfig>(() => ({
  current: pagination.current,
  size: pagination.size,
  total: pagination.total,
}));

const canCreateChannel = computed(() => hasActiveSupplier.value);

const hasActiveSearch = computed(
  () => Boolean(searchParams.q.trim()) || listFilters.value.length > 0,
);

const emptyDescription = computed(() =>
  hasActiveSearch.value
    ? $t("channels.empty.filtered")
    : $t("channels.empty.default"),
);

const {
  selectedRows,
  handleSelectionChange,
  handleDeleteCommand,
  clearSelection,
} = useTableSelection<Channel>({
  deleteApi: async (row) => {
    await deleteChannel(row.id);
  },
  getItemName: (row) => row.name,
  onDeleteSuccess: () => {
    void loadData();
  },
});

const exportItems = computed(() => [
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

const normalizeStatus = (status?: string) => {
  const value = status?.trim().toLowerCase();
  return value && allowedStatuses.has(value) ? value : "active";
};

const normalizeNumber = (value?: string) => {
  if (!value?.trim()) {
    return 0;
  }
  const num = Number(value);
  return Number.isInteger(num) && num >= 0 ? num : NaN;
};

const validateChannelImportRow = (row: Record<string, string>) => {
  if (!row.supplier_code?.trim()) {
    return { valid: false, reason: $t("channels.validation.supplierRequired") };
  }
  const code = row.channel_code?.trim() ?? "";
  if (!code) {
    return {
      valid: false,
      reason: $t("channels.validation.channelCodeRequired"),
    };
  }
  if (!/^[A-Z][A-Z0-9-]*$/.test(code)) {
    return {
      valid: false,
      reason: $t("channels.validation.channelCodePattern"),
    };
  }
  if (!row.name?.trim()) {
    return { valid: false, reason: $t("channels.validation.nameRequired") };
  }
  if (!row.path_prefix?.trim()) {
    return {
      valid: false,
      reason: $t("channels.validation.pathPrefixRequired"),
    };
  }
  const rawPrice = row.cost_price?.trim();
  if (rawPrice && !isValidCostPrice(rawPrice)) {
    return {
      valid: false,
      reason: $t("channels.validation.costPricePattern"),
    };
  }
  const rawStatus = row.status?.trim();
  if (rawStatus && !allowedStatuses.has(rawStatus.toLowerCase())) {
    return {
      valid: false,
      reason: `${$t("common.fields.status")}: ${rawStatus}`,
    };
  }
  const qpsLimit = normalizeNumber(row.qps_limit);
  if (Number.isNaN(qpsLimit)) {
    return {
      valid: false,
      reason: `${$t("channels.fields.qpsLimit")}: ${row.qps_limit}`,
    };
  }
  return { valid: true };
};

const transformChannelImportRow = (
  row: Record<string, string>,
): ChannelImportRow => {
  const supplierCode = row.supplier_code.trim();
  const rawPrice = row.cost_price?.trim();
  return {
    unique_key: `${supplierCode}:${row.channel_code.trim()}`,
    supplier_code: supplierCode,
    channel_code: row.channel_code.trim(),
    name: row.name.trim(),
    path_prefix: row.path_prefix.trim(),
    cost_price: rawPrice ? normalizeCostPriceInput(rawPrice) : "0.00",
    qps_limit: normalizeNumber(row.qps_limit) || 0,
    status: normalizeStatus(row.status),
  };
};

const checkHasActiveSupplier = async () => {
  if (!canWriteBusiness.value) {
    hasActiveSupplier.value = false;
    return;
  }
  try {
    const res = await fetchSuppliers({
      status: "active",
      page: 1,
      page_size: 1,
    });
    hasActiveSupplier.value = (res.pagination?.total ?? 0) > 0;
  } catch {
    hasActiveSupplier.value = false;
  }
};

const loadData = async () => {
  await runWithLoading(loading, async () => {
    const res = await fetchChannels({
      q: searchParams.q.trim() || undefined,
      filters: listFilters.value.length
        ? JSON.stringify(listFilters.value)
        : undefined,
      page: pagination.current,
      page_size: pagination.size,
    });
    tableData.value = res.channels;
    pagination.total = res.pagination.total;
    clearSelection();
  });
};

const handleSizeChange = (size: number) => {
  pagination.size = size;
  pagination.current = 1;
  clearSelection();
  void loadData();
};

const handleCurrentChange = (page: number) => {
  pagination.current = page;
  clearSelection();
  void loadData();
};

const handleSearch = () => {
  pagination.current = 1;
  clearSelection();
  void loadData();
};

const handleExportCommand = (command: string) => {
  const exportCommand = command as ExportCommand;
  if (exportCommand !== "page" && exportCommand !== "selected") {
    return;
  }
  const rows = exportCommand === "page" ? tableData.value : selectedRows.value;
  const csvRows: ChannelExportRow[] = rows.map((row) => ({
    supplier_code: row.supplier_code,
    channel_code: row.channel_code,
    name: row.name,
    path_prefix: row.path_prefix,
    cost_price: formatCostPrice(row.cost_price),
    qps_limit: row.qps_limit,
    status: row.status,
  }));
  exportResourceCsv(
    csvRows,
    channelExportColumns.value,
    `channels-${exportCommand}-${Date.now().toString(36)}.csv`,
  );
};

const openCreate = async () => {
  await checkHasActiveSupplier();
  if (!canCreateChannel.value) {
    ElMessage.warning($t("channels.messages.createSupplierFirst"));
    return;
  }
  editingChannel.value = null;
  formVisible.value = true;
};

const openEdit = (row: Channel) => {
  editingChannel.value = row;
  formVisible.value = true;
};

const handleView = (row: Channel) => {
  activeChannelId.value = row.id;
  detailVisible.value = true;
};

const handleFormSuccess = () => {
  void loadData();
  if (detailVisible.value && activeChannelId.value) {
    void detailDrawerRef.value?.reload();
  }
};

const handleDelete = async (row: Channel) => {
  await ElMessageBox.confirm(
    $t("channels.messages.deleteConfirm", { name: row.name }),
    $t("common.warning"),
    { type: "warning" },
  );
  await deleteChannel(row.id);
  ElMessage.success($t("common.feedback.deleteSuccess"));
  if (activeChannelId.value === row.id) {
    detailVisible.value = false;
  }
  void loadData();
};

const resolveSupplierByCode = async (
  supplierCode: string,
  cache: Map<string, Supplier>,
): Promise<Supplier | undefined> => {
  const cached = cache.get(supplierCode);
  if (cached) {
    return cached;
  }
  const res = await fetchSuppliers({
    status: "active",
    page: 1,
    page_size: 20,
    q: supplierCode,
  });
  const match = (res.suppliers ?? []).find(
    (item) => item.supplier_code === supplierCode,
  );
  if (match) {
    cache.set(supplierCode, match);
  }
  return match;
};

const handleChannelImport = async (items: Record<string, unknown>[]) => {
  const supplierCache = new Map<string, Supplier>();
  await runResourceCsvImport<ChannelImportRow>(items, async (row) => {
    const supplier = await resolveSupplierByCode(
      row.supplier_code,
      supplierCache,
    );
    if (!supplier) {
      throw new Error(
        `${$t("channels.fields.supplier")}: ${row.supplier_code}`,
      );
    }
    const created = await createChannel({
      supplier_id: supplier.id,
      channel_code: row.channel_code,
      name: row.name,
      path_prefix: row.path_prefix,
      cost_price: row.cost_price || "0.00",
      qps_limit: row.qps_limit,
    });
    if (row.status !== "active") {
      await updateChannel(created.id, { status: row.status });
    }
  });
};

const openImport = async () => {
  await checkHasActiveSupplier();
  if (!canCreateChannel.value) {
    ElMessage.warning($t("channels.messages.createSupplierFirst"));
    return;
  }
  importVisible.value = true;
};

watch(
  () => props.activeTab,
  (val) => {
    if (val === "channels") {
      void checkHasActiveSupplier();
    }
  },
);

onMounted(() => {
  void loadData();
  void checkHasActiveSupplier();
});

defineExpose({ loadData, checkHasActiveSupplier });
</script>

<style scoped lang="scss">
.channel-page {
  padding: 0;
}
</style>
