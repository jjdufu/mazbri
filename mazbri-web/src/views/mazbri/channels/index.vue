<template>
  <div ref="pageRef" class="channel-page" style="display: flex; flex-direction: column; flex: 1; min-height: 0;">
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
          :show-import="true"
          :import-label="$t('common.actions.import')"
          :import-disabled="!canCreateChannel"
          :show-export="true"
          :export-items="exportItems"
          :export-disabled="tableData.length === 0"
          :show-primary="true"
          :primary-label="$t('channels.actions.add')"
          :primary-disabled="!canCreateChannel"
          :show-delete="true"
          :delete-total-count="tableData.length"
          :delete-selected-count="selectedRows.length"
          @search="handleSearch"
          @import="openImport"
          @export="handleExportCommand"
          @primary="openCreate"
          @delete="(type) => handleDeleteCommand(type, tableData)"
        >
          <template #import="{ click, disabled, icon, label }">
            <ElTooltip
              :disabled="canCreateChannel"
              :content="$t('channels.messages.createSupplierFirst')"
              placement="top"
            >
              <span class="channel-action-trigger">
                <ElButton :disabled="disabled" @click="click">
                  <template #icon><ArtSvgIcon :icon="icon" /></template>
                  {{ label }}
                </ElButton>
              </span>
            </ElTooltip>
          </template>

          <template #primary="{ click, disabled, icon, label, type }">
            <ElTooltip
              :disabled="canCreateChannel"
              :content="$t('channels.messages.createSupplierFirst')"
              placement="top"
            >
              <span class="channel-action-trigger">
                <ElButton :disabled="disabled" :type="type" @click="click">
                  <template #icon><ArtSvgIcon :icon="icon" /></template>
                  {{ label }}
                </ElButton>
              </span>
            </ElTooltip>
          </template>
        </ResourceTableActionBar>
      </template>

      <template #supplier="{ row }">
        {{ row.supplier_name }} ({{ row.supplier_code }})
      </template>
      <template #path_prefix="{ row }">
        {{ row.path_prefix }}
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
          <ElButton link type="danger" @click="handleDelete(row)">{{
            $t("common.actions.delete")
          }}</ElButton>
        </TableActionRow>
      </template>
    </ResourceTableShell>


    <EditDialog
      v-model="dialogVisible"
      :title="$t('channels.dialog.createTitle')"
      width="680px"
      :saving="submitting"
      @confirm="submitForm"
    >
      <ElForm ref="formRef" :model="form" :rules="rules" label-width="120px">
        <ElFormItem :label="$t('channels.fields.supplier')" prop="supplier_id">
          <ElSelect
            v-model="form.supplier_id"
            filterable
            :placeholder="$t('channels.placeholders.supplier')"
          >
            <ElOption
              v-for="supplier in suppliers"
              :key="supplier.id"
              :label="`${supplier.name} (${supplier.supplier_code})`"
              :value="supplier.id"
            />
          </ElSelect>
        </ElFormItem>
        <ElFormItem
          :label="$t('channels.fields.channelCode')"
          prop="channel_code"
        >
          <ElInput
            v-model="form.channel_code"
            :placeholder="$t('channels.placeholders.channelCode')"
          />
        </ElFormItem>
        <ElFormItem :label="$t('channels.fields.name')" prop="name">
          <ElInput
            v-model="form.name"
            :placeholder="$t('channels.placeholders.name')"
          />
        </ElFormItem>
        <ElFormItem
          :label="$t('channels.fields.pathPrefix')"
          prop="path_prefix"
        >
          <ElInput
            v-model="form.path_prefix"
            :placeholder="$t('channels.placeholders.pathPrefix')"
          />
        </ElFormItem>
        <ElFormItem :label="$t('channels.fields.costPrice')">
          <ElInput
            v-model="form.cost_price"
            :placeholder="$t('channels.placeholders.costPrice')"
          />
        </ElFormItem>
        <ElFormItem :label="$t('channels.fields.qpsLimit')">
          <ElInputNumber v-model="form.qps_limit" :min="0" />
        </ElFormItem>
      </ElForm>
    </EditDialog>

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
import {
  ElMessage,
  ElMessageBox,
  type FormInstance,
  type FormRules,
} from "element-plus";
import { $t } from "@/locales";
import {
  createChannel,
  deleteChannel,
  fetchChannels,
  updateChannel,
  type Channel,
} from "@/api/route-management";
import { fetchSuppliers, type Supplier } from "@/api/supplier";
import EditDialog from "@/components/common/EditDialog.vue";
import ImportDialog from "@/components/common/ImportDialog.vue";
import StatusIndicator from "@/components/common/StatusIndicator.vue";
import ResourceTableShell from "@/components/common/resource-table-shell/index.vue";
import ResourceTableActionBar from "@/components/common/resource-table-action-bar/index.vue";
import { channelsCatalog } from "@/list-catalog/common-resources";
import type { ListFilter } from "@/types/list-query";
import { runWithLoading } from "@/composables/useAsyncState";
import { useAdaptiveTableColumns } from "@/composables/useAdaptiveTableColumns";
import {
  exportResourceCsv,
  runResourceCsvImport,
} from "@/composables/useResourceCsvImportExport";
import { useTableSelection } from "@/composables/useTableSelection";
import type { ColumnConfig } from "@/composables/useResourceCsvImportExport";
import type { ColumnOption, PaginationConfig } from "@/types/component";
import { useRouter } from "vue-router";
defineOptions({ name: "UpstreamChannelList" });
const router = useRouter();
const props = defineProps<{
  activeTab?: string;
}>();

watch(
  () => props.activeTab,
  (val) => {
    if (val === "channels") {
      void loadSuppliers();
    }
  },
);
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
const allowedStatuses = new Set(["active", "inactive", "suspended"]);
const pageRef = ref<HTMLElement | null>(null);
const loading = ref(false);
const submitting = ref(false);
const dialogVisible = ref(false);
const importVisible = ref(false);
const tableData = ref<Channel[]>([]);
const suppliers = ref<Supplier[]>([]);
const formRef = ref<FormInstance>();
const searchParams = reactive({ q: "" });
const listFilters = ref<ListFilter[]>([]);
const pagination = reactive({
  current: 1,
  size: 20,
  total: 0,
});
const columns = ref<ColumnOption<Channel>[]>([]);
const channelColumns: ColumnOption<Channel>[] = [
  { type: "selection", width: 55 },
  {
    prop: "channel_code",
    label: $t("channels.fields.channelCode"),
    minWidth: 100,
  },
  { prop: "name", label: $t("channels.fields.name"), minWidth: 120 },
  {
    prop: "supplier",
    label: $t("channels.fields.supplier"),
    minWidth: 120,
    useSlot: true,
  },
  {
    prop: "path_prefix",
    label: $t("channels.fields.pathPrefix"),
    minWidth: 120,
    useSlot: true,
  },
  { prop: "qps_limit", label: $t("channels.fields.qpsLimit"), width: 90 },
  {
    prop: "status",
    label: $t("common.fields.status"),
    width: 110,
    useSlot: true,
  },
  {
    prop: "operations",
    label: $t("common.fields.operations"),
    width: 170,
    fixed: "right",
    useSlot: true,
  },
];
const channelImportColumns: ColumnConfig[] = [
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
  { key: "qps_limit", label: $t("channels.fields.qpsLimit"), required: false },
  { key: "status", label: $t("common.fields.status"), required: false },
];
const channelExportColumns: { key: keyof ChannelExportRow; label: string }[] = [
  { key: "supplier_code", label: $t("channels.fields.supplier") },
  { key: "channel_code", label: $t("channels.fields.channelCode") },
  { key: "name", label: $t("channels.fields.name") },
  { key: "path_prefix", label: $t("channels.fields.pathPrefix") },
  { key: "cost_price", label: $t("channels.fields.costPrice") },
  { key: "qps_limit", label: $t("channels.fields.qpsLimit") },
  { key: "status", label: $t("common.fields.status") },
];
const channelImportRows = ref<Channel[]>([]);
const channelImportExistingData = computed(() =>
  channelImportRows.value.map((row) => ({
    unique_key: `${row.supplier_code}:${row.channel_code}`,
  })),
);
const getChannelCellText = (columnKey: string, row: Channel) => {
  switch (columnKey) {
    case "channel_code":
      return row.channel_code || "-";
    case "name":
      return row.name || "-";
    case "supplier":
      return `${row.supplier_name || "-"} (${row.supplier_code || "-"})`;
    case "path_prefix":
      return row.path_prefix || "-";
    case "qps_limit":
      return String(row.qps_limit ?? "-");
    case "status":
      return $t(`channels.status.${row.status}`);
    case "operations":
      return `${$t("common.actions.details")} ${$t("common.actions.delete")}`;
    default:
      return row.name || row.channel_code || "-";
  }
};
const getChannelColumnWeight = (columnKey: string) => {
  switch (columnKey) {
    case "channel_code":
      return 1.1;
    case "name":
      return 1.4;
    case "supplier":
      return 2;
    case "path_prefix":
      return 1.8;
    case "qps_limit":
      return 0.8;
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
const channelImportFormatHint = computed(() =>
  [
    $t("feedback.import.csvFormatHint"),
    $t("feedback.import.requiredColumns", {
      columns: channelImportColumns
        .filter((column) => column.required)
        .map((column) => column.key)
        .join(", "),
    }),
  ].join("\n"),
);
const form = reactive({
  supplier_id: "",
  channel_code: "",
  name: "",
  path_prefix: "",
  cost_price: "0",
  qps_limit: 0,
  status: "active",
});
const rules: FormRules = {
  supplier_id: [
    {
      required: true,
      message: $t("channels.validation.supplierRequired"),
      trigger: "change",
    },
  ],
  channel_code: [
    {
      required: true,
      message: $t("channels.validation.channelCodeRequired"),
      trigger: "blur",
    },
    {
      pattern: /^[A-Z][A-Z0-9-]*$/,
      message: $t("channels.validation.channelCodePattern"),
      trigger: "blur",
    },
    {
      validator: (
        _rule: unknown,
        value: unknown,
        callback: (error?: Error) => void,
      ) => {
        const exists = tableData.value.some((c) => c.channel_code === value);
        if (exists) {
          callback(new Error($t("channels.validation.channelCodeDuplicate")));
        } else {
          callback();
        }
      },
      trigger: "blur",
    },
  ],
  name: [
    {
      required: true,
      message: $t("channels.validation.nameRequired"),
      trigger: "blur",
    },
  ],
  path_prefix: [
    {
      required: true,
      message: $t("channels.validation.pathPrefixRequired"),
      trigger: "blur",
    },
  ],
};
const artPagination = computed<PaginationConfig>(() => ({
  current: pagination.current,
  size: pagination.size,
  total: pagination.total,
}));
const canCreateChannel = computed(() => suppliers.value.length > 0);
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
interface ChannelExportRow extends Record<string, unknown> {
  supplier_code: string;
  channel_code: string;
  name: string;
  path_prefix: string;
  cost_price: string;
  qps_limit: number;
  status: string;
}
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
const resetForm = () => {
  Object.assign(form, {
    supplier_id: "",
    channel_code: "",
    name: "",
    path_prefix: "",
    cost_price: "0",
    qps_limit: 0,
    status: "active",
  });
  formRef.value?.clearValidate();
};
const normalizeStatus = (status?: string) => {
  const value = status?.trim().toLowerCase();
  return value && allowedStatuses.has(value) ? value : "active";
};
const normalizeNumber = (value?: string) => {
  if (!value?.trim()) {
    return 0;
  }
  const num = Number(value);
  return Number.isFinite(num) ? num : NaN;
};
const validateChannelImportRow = (row: Record<string, string>) => {
  if (!row.supplier_code?.trim()) {
    return { valid: false, reason: $t("channels.validation.supplierRequired") };
  }
  if (!row.channel_code?.trim()) {
    return {
      valid: false,
      reason: $t("channels.validation.channelCodeRequired"),
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
  const supplier = suppliers.value.find(
    (item) => item.supplier_code === row.supplier_code.trim(),
  );
  if (!supplier) {
    return {
      valid: false,
      reason: `${$t("channels.fields.supplier")}: ${row.supplier_code}`,
    };
  }
  return { valid: true };
};
const transformChannelImportRow = (
  row: Record<string, string>,
): ChannelImportRow => {
  const supplierCode = row.supplier_code.trim();
  return {
    unique_key: `${supplierCode}:${row.channel_code.trim()}`,
    supplier_code: supplierCode,
    channel_code: row.channel_code.trim(),
    name: row.name.trim(),
    path_prefix: row.path_prefix.trim(),
    cost_price: row.cost_price?.trim() || "0",
    qps_limit: normalizeNumber(row.qps_limit) || 0,
    status: normalizeStatus(row.status),
  };
};
const resolveSupplier = (supplierCode: string) =>
  suppliers.value.find((item) => item.supplier_code === supplierCode);
const loadSuppliers = async () => {
  try {
    const res = await fetchSuppliers({ status: "active", page_size: 10000 });
    suppliers.value = res.suppliers;
  } catch {
    suppliers.value = [];
  }
};
const loadChannelImportRows = async () => {
  try {
    const res = await fetchChannels({ page_size: 10000 });
    channelImportRows.value = res.channels;
  } catch {
    channelImportRows.value = tableData.value;
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
  loadData();
};
const handleCurrentChange = (page: number) => {
  pagination.current = page;
  clearSelection();
  loadData();
};
const handleSearch = () => {
  pagination.current = 1;
  clearSelection();
  loadData();
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
    cost_price: row.cost_price || "0",
    qps_limit: row.qps_limit,
    status: row.status,
  }));
  exportResourceCsv(
    csvRows,
    channelExportColumns,
    `channels-${exportCommand}-${Date.now().toString(36)}.csv`,
  );
};
const openCreate = async () => {
  await loadSuppliers();
  if (!canCreateChannel.value) {
    ElMessage.warning($t("channels.messages.createSupplierFirst"));
    return;
  }
  resetForm();
  dialogVisible.value = true;
};
const handleView = (row: Channel) => {
  router.push({ name: "ChannelDetail", params: { id: row.id } });
};
const submitForm = async () => {
  const valid = await formRef.value?.validate().catch(() => false);
  if (!valid) {
    return;
  }
  await runWithLoading(submitting, async () => {
    const { status: _status, ...payload } = form;
    await createChannel(payload);
    dialogVisible.value = false;
    ElMessage.success($t("common.feedback.saveSuccess"));
    loadData();
  });
};
const handleDelete = async (row: Channel) => {
  await ElMessageBox.confirm(
    $t("channels.messages.deleteConfirm", { name: row.name }),
    $t("common.warning"),
    { type: "warning" },
  );
  await deleteChannel(row.id);
  ElMessage.success($t("common.feedback.deleteSuccess"));
  loadData();
};
const handleChannelImport = async (items: Record<string, unknown>[]) => {
  await runResourceCsvImport<ChannelImportRow>(items, async (row) => {
    const supplier = resolveSupplier(row.supplier_code);
    if (!supplier) {
      throw new Error("supplier not found");
    }
    const created = await createChannel({
      supplier_id: supplier.id,
      channel_code: row.channel_code,
      name: row.name,
      path_prefix: row.path_prefix,
      cost_price: row.cost_price || "0",
      qps_limit: row.qps_limit,
    });
    if (row.status !== "active") {
      await updateChannel(created.id, { status: row.status });
    }
  });
};
const openImport = async () => {
  await loadSuppliers();
  if (!canCreateChannel.value) {
    ElMessage.warning($t("channels.messages.createSupplierFirst"));
    return;
  }
  await loadChannelImportRows();
  importVisible.value = true;
};
onMounted(() => {
  loadData();
  loadSuppliers();
});
</script>

<style scoped lang="scss">
.channel-action-trigger {
  display: inline-block;
}

.qps-limit-field {
  width: 100%;

  .field-hint {
    margin: 6px 0 0;
    font-size: 12px;
    line-height: 1.5;
    color: var(--el-text-color-secondary);
  }
}
</style>
