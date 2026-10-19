<template>
  <div
    ref="pageRef"
    class="supplier-page"
    style="display: flex; flex-direction: column; flex: 1; min-height: 0"
  >
    <ResourceTableShell
      v-model:columns="columns"
      :title="$t('supplier.list.title')"
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
          :filter-catalog="suppliersCatalog"
          :show-import="true"
          :import-label="$t('common.actions.import')"
          :show-export="true"
          :export-items="exportItems"
          :export-disabled="tableData.length === 0"
          :show-primary="true"
          :primary-label="$t('upstreamConnector.codeFreeAccess')"
          :primary-icon="'ri:plug-line'"
          :primary-button-type="'primary'"
          :primary-roles="['R_ADMIN', 'R_OPERATOR']"
          :show-delete="true"
          :delete-total-count="tableData.length"
          :delete-selected-count="selectedRows.length"
          @search="handleSearch"
          @import="openImport"
          @export="handleExportCommand"
          @primary="openCodeFreeAccess"
          @delete="(type) => handleDeleteCommand(type, tableData)"
        >

          <template #before-delete>
            <ElButton
              v-roles="['R_ADMIN', 'R_OPERATOR']"
              type="success"
              @click="emit('access')"
            >
              <template #icon>
                <ArtSvgIcon icon="ri:add-line" />
              </template>
              {{ $t("upstreamOnboarding.mode.microservice") }}
            </ElButton>
          </template>
        </ResourceTableActionBar>
      </template>

      <template #upstream_url="{ row }">
        {{ formatUrlDisplay(row.upstream_url) }}
      </template>
      <template #upstream_type="{ row }">
        <ElTag
          size="small"
          effect="plain"
          :type="resolveUpstreamType(row) === 'transparent' ? 'primary' : 'info'"
        >
          {{
            resolveUpstreamType(row) === "transparent"
              ? $t("supplier.upstreamOptions.transparent")
              : $t("supplier.upstreamOptions.microservice")
          }}
        </ElTag>
      </template>
      <template #status="{ row }">
        <StatusIndicator :type="row.status === 'active' ? 'success' : 'info'">
          {{ row.status }}
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

    <ImportDialog
      v-model="importVisible"
      :title="$t('common.actions.import')"
      :columns="supplierImportColumns"
      :format-hint="supplierImportFormatHint"
      unique-key="supplier_code"
      :existing-data="supplierImportExistingData"
      :validator="validateSupplierImportRow"
      :transformer="transformSupplierImportRow"
      :on-import="handleSupplierImport"
      :show-success-message="false"
      :show-error-message="false"
      @success="loadData"
    />
  </div>
</template>

<script setup lang="ts">
import { ElMessage, ElMessageBox } from "element-plus";
import {
  createSupplier,
  deleteSupplier,
  fetchSuppliers,
  updateSupplier,
  type Supplier,
} from "@/api/supplier";
import ImportDialog from "@/components/common/ImportDialog.vue";
import StatusIndicator from "@/components/common/StatusIndicator.vue";
import ResourceTableShell from "@/components/common/resource-table-shell/index.vue";
import ResourceTableActionBar from "@/components/common/resource-table-action-bar/index.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { useAdaptiveTableColumns } from "@/composables/useAdaptiveTableColumns";
import {
  exportResourceCsv,
  runResourceCsvImport,
} from "@/composables/useResourceCsvImportExport";
import { useTableSelection } from "@/composables/useTableSelection";
import type { ColumnConfig } from "@/composables/useResourceCsvImportExport";
import { suppliersCatalog } from "@/list-catalog/common-resources";
import type { ListFilter } from "@/types/list-query";
import type { ColumnOption, PaginationConfig } from "@/types/component";
import { $t } from "@/locales";
import { formatUrlDisplay, stripUrlScheme } from "@/utils/sensitive";
import { resolveUpstreamType } from "@/views/mazbri/suppliers/supplier-form";
import { ref, reactive, computed, onMounted } from "vue";
import { useRouter } from "vue-router";

defineOptions({ name: "SupplierList" });
const emit = defineEmits<{ access: [] }>();
const router = useRouter();
type ExportCommand = "page" | "selected";
interface SupplierImportRow extends Record<string, unknown> {
  supplier_code: string;
  name: string;
  upstream_url: string;
  status: string;
}
interface SupplierExportRow extends Record<string, unknown> {
  supplier_code: string;
  name: string;
  upstream_url: string;
  status: string;
}
const allowedStatuses = new Set(["active", "inactive", "suspended"]);
const pageRef = ref<HTMLElement | null>(null);
const loading = ref(false);
const importVisible = ref(false);
const tableData = ref<Supplier[]>([]);
const searchParams = reactive({ q: "" });
const listFilters = ref<ListFilter[]>([]);
const pagination = reactive({
  current: 1,
  size: 20,
  total: 0,
});
const columns = ref<ColumnOption<Supplier>[]>([]);
const supplierColumns: ColumnOption<Supplier>[] = [
  { type: "selection", width: 55 },
  { prop: "supplier_code", label: $t("supplier.fields.code"), minWidth: 112 },
  { prop: "name", label: $t("supplier.fields.name"), minWidth: 120 },
  {
    prop: "upstream_type",
    label: $t("supplier.fields.upstreamType"),
    width: 110,
    useSlot: true,
  },
  {
    prop: "upstream_url",
    label: $t("supplier.fields.accessAddress"),
    minWidth: 140,
    useSlot: true,
  },
  {
    prop: "status",
    label: $t("supplier.fields.status"),
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
const getSupplierCellText = (columnKey: string, row: Supplier) => {
  switch (columnKey) {
    case "supplier_code":
      return row.supplier_code || "-";
    case "name":
      return row.name || "-";
    case "upstream_type":
      return resolveUpstreamType(row) === "transparent"
        ? $t("supplier.upstreamOptions.transparent")
        : $t("supplier.upstreamOptions.microservice");
    case "upstream_url":
      return formatUrlDisplay(row.upstream_url);
    case "status":
      return row.status || "-";
    case "operations":
      return `${$t("common.actions.details")} ${$t("common.actions.delete")}`;
    default:
      return row.name || "-";
  }
};
const getSupplierColumnWeight = (columnKey: string) => {
  switch (columnKey) {
    case "supplier_code":
      return 1.1;
    case "name":
      return 1.1;
    case "upstream_type":
      return 0.7;
    case "upstream_url":
      return 2.2;
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
  baseColumns: supplierColumns,
  getCellText: ({ columnKey, row }) => getSupplierCellText(columnKey, row),
  getColumnWeight: ({ columnKey }) => getSupplierColumnWeight(columnKey),
});
const supplierImportColumns: ColumnConfig[] = [
  { key: "supplier_code", label: $t("supplier.fields.code"), required: true },
  { key: "name", label: $t("supplier.fields.name"), required: true },
  {
    key: "upstream_url",
    label: $t("supplier.fields.accessAddress"),
    required: true,
  },
  { key: "status", label: $t("supplier.fields.status"), required: false },
];
const supplierExportColumns: { key: keyof SupplierExportRow; label: string }[] =
  [
    { key: "supplier_code", label: $t("supplier.fields.code") },
    { key: "name", label: $t("supplier.fields.name") },
    { key: "upstream_url", label: $t("supplier.fields.accessAddress") },
    { key: "status", label: $t("supplier.fields.status") },
  ];
const supplierImportRows = ref<Supplier[]>([]);
const supplierImportExistingData = computed(() =>
  supplierImportRows.value.map((row) => ({
    supplier_code: row.supplier_code,
  })),
);
const supplierImportFormatHint = computed(() =>
  [
    $t("feedback.import.csvFormatHint"),
    $t("feedback.import.requiredColumns", {
      columns: supplierImportColumns
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
const hasActiveSearch = computed(
  () => Boolean(searchParams.q.trim()) || listFilters.value.length > 0,
);
const emptyDescription = computed(() =>
  hasActiveSearch.value
    ? $t("supplier.empty.noMatch")
    : $t("supplier.empty.noData"),
);
const {
  selectedRows,
  handleSelectionChange,
  handleDeleteCommand,
  clearSelection,
} = useTableSelection<Supplier>({
  deleteApi: async (row) => {
    await deleteSupplier(row.id);
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
const validateSupplierImportRow = (row: Record<string, string>) => {
  if (!row.supplier_code?.trim()) {
    return { valid: false, reason: $t("supplier.validation.codeRequired") };
  }
  if (!row.name?.trim()) {
    return { valid: false, reason: $t("supplier.validation.nameRequired") };
  }
  if (!row.upstream_url?.trim()) {
    return {
      valid: false,
      reason: $t("supplier.validation.accessAddressRequired"),
    };
  }
  const rawStatus = row.status?.trim();
  if (rawStatus && !allowedStatuses.has(rawStatus.toLowerCase())) {
    return {
      valid: false,
      reason: `${$t("supplier.fields.status")}: ${rawStatus}`,
    };
  }
  return { valid: true };
};
const transformSupplierImportRow = (
  row: Record<string, string>,
): SupplierImportRow => ({
  supplier_code: row.supplier_code.trim(),
  name: row.name.trim(),
  upstream_url: stripUrlScheme(row.upstream_url.trim()),
  status: normalizeStatus(row.status),
});
const loadSupplierImportRows = async () => {
  try {
    const res = await fetchSuppliers({ page_size: 10000 });
    supplierImportRows.value = res.suppliers;
  } catch {
    supplierImportRows.value = tableData.value;
  }
};
const loadData = async () => {
  await runWithLoading(loading, async () => {
    const res = await fetchSuppliers({
      q: searchParams.q.trim() || undefined,
      filters: listFilters.value.length
        ? JSON.stringify(listFilters.value)
        : undefined,
      page: pagination.current,
      page_size: pagination.size,
    });
    tableData.value = res.suppliers;
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
  const csvRows: SupplierExportRow[] = rows.map((row) => ({
    supplier_code: row.supplier_code,
    name: row.name,
    upstream_url: stripUrlScheme(row.upstream_url),
    status: row.status,
  }));
  exportResourceCsv(
    csvRows,
    supplierExportColumns,
    `suppliers-${exportCommand}-${Date.now().toString(36)}.csv`,
  );
};

const handleView = (row: Supplier) => {
  router.push({ name: "SupplierDetail", params: { id: row.id } });
};


const openCodeFreeAccess = () => {
  router.push({ name: "UpstreamConnectorWorkbench" });
};

const handleDelete = async (row: Supplier) => {
  await ElMessageBox.confirm(
    $t("supplier.messages.deleteConfirm", { name: row.name }),
    $t("common.tips"),
    { type: "warning" },
  );
  await deleteSupplier(row.id);
  ElMessage.success($t("common.feedback.deleteSuccess"));
  loadData();
};
const handleSupplierImport = async (items: Record<string, unknown>[]) => {
  await runResourceCsvImport<SupplierImportRow>(items, async (row) => {
    const created = await createSupplier({
      supplier_code: row.supplier_code,
      name: row.name,
      upstream_url: row.upstream_url,
      config: { upstream_type: "microservice" },
    });
    if (row.status !== "active") {
      await updateSupplier(created.id, { status: row.status });
    }
  });
};
const openImport = async () => {
  await loadSupplierImportRows();
  importVisible.value = true;
};
onMounted(loadData);
defineExpose({ loadData });
</script>

<style scoped lang="scss">
.supplier-page {
  padding: 0;
}
</style>
