<template>
  <div ref="pageRef" class="product-list-page" style="display: flex; flex-direction: column; flex: 1; min-height: 0;">
    <ResourceTableShell
      v-model:columns="columns"
      :title="$t('product.list.title')"
      :data="tableData"
      :loading="loading"
      :empty-text="emptyDescription"
      :pagination="artPagination"
      :fit="true"
      table-layout="auto"
      @refresh="loadData"
      @selection-change="handleSelectionChange"
      @pagination:size-change="handlePageSizeChange"
      @pagination:current-change="handleCurrentPageChange"
    >
      <template #actions>
        <ResourceTableActionBar
          v-model="searchKeyword"
          v-model:filters="listFilters"
          :filter-catalog="productsCatalog"
          :show-import="true"
          :import-label="$t('common.actions.import')"
          :import-roles="['R_ADMIN', 'R_FINANCE']"
          :export-items="exportItems"
          :export-disabled="tableData.length === 0"
          :show-primary="true"
          :primary-label="$t('product.actions.add')"
          :primary-roles="['R_ADMIN', 'R_FINANCE']"
          :show-delete="true"
          :delete-total-count="tableData.length"
          :delete-selected-count="selectedRows.length"
          :delete-roles="['R_ADMIN', 'R_FINANCE']"
          @search="handleSearch"
          @import="openImport"
          @export="handleExportCommand"
          @primary="handleCreate"
          @delete="(type) => handleDeleteCommand(type, tableData)"
        />
      </template>

      <template #enabled="{ row }">
        <StatusIndicator :type="row.enabled ? 'success' : 'info'">
          {{
            row.enabled
              ? $t("common.status.enabled")
              : $t("common.status.disabled")
          }}
        </StatusIndicator>
      </template>
      <template #created_at="{ row }">
        {{ formatDate(row.created_at) }}
      </template>
      <template #operations="{ row }">
        <TableActionRow>
          <ElButton link type="primary" @click="handleView(row)">{{
            $t("common.actions.details")
          }}</ElButton>
          <ElButton
            v-roles="['R_ADMIN', 'R_FINANCE']"
            link
            type="danger"
            @click="handleDelete(row)"
            >{{ $t("common.actions.delete") }}</ElButton
          >
        </TableActionRow>
      </template>
    </ResourceTableShell>

    <CreateDialog
      v-model="createVisible"
      :reservation="activeReservation"
      @success="handleCreateSuccess"
    />

    <ImportDialog
      v-model="importVisible"
      :title="$t('common.actions.import')"
      :columns="productImportColumns"
      :format-hint="productImportFormatHint"
      unique-key="name"
      :existing-data="productImportExistingData"
      :validator="validateProductImportRow"
      :transformer="transformProductImportRow"
      :on-import="handleProductImport"
      :show-success-message="false"
      :show-error-message="false"
      @success="loadData"
    />
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { ElMessageBox, ElMessage } from "element-plus";
import { $t } from "@/locales";
import type {
  Product,
  ProductIdReservation,
  ProductListQuery,
} from "@/types/api/product";
import {
  createProductReservation,
  createProduct,
  fetchProductList,
  deleteProduct,
  releaseProductReservation,
} from "@/api/product";
import CreateDialog from "./modules/create-dialog.vue";
import StatusIndicator from "@/components/common/StatusIndicator.vue";
import ResourceTableShell from "@/components/common/resource-table-shell/index.vue";
import ImportDialog from "@/components/common/ImportDialog.vue";
import ResourceTableActionBar from "@/components/common/resource-table-action-bar/index.vue";
import { productsCatalog } from "@/list-catalog/common-resources";
import type { ListFilter } from "@/types/list-query";
import { runWithLoading } from "@/composables/useAsyncState";
import { useAdaptiveTableColumns } from "@/composables/useAdaptiveTableColumns";
import {
  exportResourceCsv,
  runResourceCsvImport,
} from "@/composables/useResourceCsvImportExport";
import { useTableSelection } from "@/composables/useTableSelection";
import type { ColumnConfig } from "@/composables/useResourceCsvImportExport";
import { logger } from "@/utils/sys";
import { formatDateTime as formatDate } from "@/utils/format";
import type { ColumnOption, PaginationConfig } from "@/types/component";
defineOptions({ name: "ProductList" });
type ExportCommand = "page" | "selected";
interface ProductImportRow extends Record<string, unknown> {
  name: string;
  enabled?: string;
}
interface ProductExportRow extends Record<string, unknown> {
  product_id: string;
  name: string;
  enabled: boolean;
  created_at: string;
}
const router = useRouter();
const pageRef = ref<HTMLElement | null>(null);
const loading = ref(false);
const tableData = ref<Product[]>([]);
const createVisible = ref(false);
const importVisible = ref(false);
const activeReservation = ref<ProductIdReservation | null>(null);
const reservationConsumed = ref(false);
const importProducts = ref<Product[]>([]);
const columns = ref<ColumnOption<Product>[]>([]);
const productColumns: ColumnOption<Product>[] = [
  { type: "selection", width: 55 },
  { prop: "product_id", label: $t("product.columns.productId"), width: 180 },
  { prop: "name", label: $t("product.columns.name"), minWidth: 100 },
  {
    prop: "enabled",
    label: $t("product.columns.enabled"),
    width: 100,
    useSlot: true,
  },
  {
    prop: "created_at",
    label: $t("common.fields.createdAt"),
    width: 180,
    useSlot: true,
  },
  {
    prop: "operations",
    label: $t("common.fields.operations"),
    width: 200,
    fixed: "right",
    useSlot: true,
  },
];
const searchForm = reactive<ProductListQuery>({
  q: "",
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
  hasActiveSearch.value
    ? $t("product.empty.filtered")
    : $t("product.empty.default"),
);
const getProductCellText = (columnKey: string, row: Product) => {
  switch (columnKey) {
    case "product_id":
      return row.product_id || "-";
    case "enabled":
      return row.enabled
        ? $t("common.status.enabled")
        : $t("common.status.disabled");
    case "created_at":
      return formatDate(row.created_at);
    case "operations":
      return $t("common.actions.details");
    default:
      return row.name || "-";
  }
};
const getProductColumnWeight = (columnKey: string) => {
  switch (columnKey) {
    case "product_id":
      return 1.4;
    case "name":
      return 1.2;
    case "enabled":
      return 0.9;
    case "created_at":
      return 1.2;
    default:
      return 1;
  }
};
useAdaptiveTableColumns({
  columns,
  containerRef: pageRef,
  rows: tableData,
  baseColumns: productColumns,
  getCellText: ({ columnKey, row }) => getProductCellText(columnKey, row),
  getColumnWeight: ({ columnKey }) => getProductColumnWeight(columnKey),
});
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
const { selectedRows, handleSelectionChange, handleDeleteCommand } =
  useTableSelection<Product>({
    deleteApi: (row) => deleteProduct(row.product_id),
    getItemName: (row) => row.name,
    onDeleteSuccess: () => loadData(),
  });
const productImportColumns: ColumnConfig[] = [
  { key: "name", label: $t("product.fields.name"), required: true },
  { key: "enabled", label: $t("product.fields.enabled"), required: false },
];
const productExportColumns: { key: keyof ProductExportRow; label: string }[] = [
  { key: "product_id", label: $t("product.fields.productId") },
  { key: "name", label: $t("product.fields.name") },
  { key: "enabled", label: $t("product.fields.enabled") },
  { key: "created_at", label: $t("common.fields.createdAt") },
];
const productImportExistingData = computed(() =>
  importProducts.value.map((product) => ({
    name: product.name,
  })),
);
const productImportFormatHint = computed(() =>
  [
    $t("feedback.import.csvFormatHint"),
    $t("feedback.import.requiredColumns", {
      columns: productImportColumns
        .filter((column) => column.required)
        .map((column) => column.key)
        .join(", "),
    }),
  ].join("\n"),
);
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
const parseBoolean = (value?: string) => {
  const normalized = value?.trim().toLowerCase();
  if (!normalized) {
    return undefined;
  }
  if (["true", "1", "yes", "y", "enabled", "on"].includes(normalized)) {
    return true;
  }
  if (["false", "0", "no", "n", "disabled", "off"].includes(normalized)) {
    return false;
  }
  return undefined;
};
const loadData = async () => {
  await runWithLoading(loading, async () => {
    const res = await fetchProductList({
      q: searchForm.q,
      filters: listFilters.value.length
        ? JSON.stringify(listFilters.value)
        : undefined,
      page: pagination.page,
      limit: pagination.limit,
    });
    tableData.value = res.products;
    pagination.total = res.pagination?.total ?? 0;
  }).catch((error) => {
    logger.error("Failed to load product list:", error);
  });
};
const loadImportProducts = async () => {
  try {
    const res = await fetchProductList();
    importProducts.value = res.products;
  } catch (error) {
    logger.error("Failed to load import product list:", error);
    importProducts.value = tableData.value;
  }
};
const handlePageSizeChange = (size: number) => {
  pagination.limit = size;
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

const handleCreate = async () => {
  try {
    activeReservation.value = await createProductReservation();
    reservationConsumed.value = false;
    createVisible.value = true;
  } catch (error) {
    logger.error("Failed to reserve product_id:", error);
  }
};
const releaseActiveReservation = async () => {
  const reservation = activeReservation.value;
  if (!reservation) return;
  try {
    await releaseProductReservation(reservation.id);
  } catch (error) {
    logger.error("Failed to release product_id reservation:", error);
  } finally {
    activeReservation.value = null;
    reservationConsumed.value = false;
  }
};
const handleCreateSuccess = async () => {
  reservationConsumed.value = true;
  activeReservation.value = null;
  await loadData();
};
const handleView = (row: Product) => {
  router.push({ name: "ProductDetail", params: { id: row.product_id } });
};
const handleDelete = async (row: Product) => {
  try {
    await ElMessageBox.confirm(
      $t("product.messages.deleteProductConfirm", { name: row.name }),
      $t("common.tips"),
      {
        confirmButtonText: $t("common.confirm"),
        cancelButtonText: $t("common.cancel"),
        type: "warning",
      },
    );
    await deleteProduct(row.product_id);
    ElMessage.success($t("common.feedback.deleteSuccess"));
    loadData();
  } catch (error) {
    if (error !== "cancel") {
      logger.error("Failed to delete product:", error);
    }
  }
};
const validateProductImportRow = (row: Record<string, string>) => {
  if (!row.name?.trim()) {
    return {
      valid: false,
      reason: $t("product.createDialog.validation.nameRequired"),
    };
  }
  const trimmedName = row.name.trim();
  if (trimmedName.length < 2 || trimmedName.length > 100) {
    return {
      valid: false,
      reason: $t("product.createDialog.validation.nameLength"),
    };
  }
  return { valid: true };
};
const transformProductImportRow = (
  row: Record<string, string>,
): ProductImportRow => ({
  name: row.name.trim(),
  enabled: row.enabled?.trim() || "",
});
const openImport = async () => {
  await loadImportProducts();
  importVisible.value = true;
};
const handleExportCommand = (command: string) => {
  const exportCommand = command as ExportCommand;
  if (exportCommand !== "page" && exportCommand !== "selected") {
    return;
  }
  const rows = exportCommand === "page" ? tableData.value : selectedRows.value;
  const csvRows: ProductExportRow[] = rows.map((row) => ({
    product_id: row.product_id,
    name: row.name,
    enabled: row.enabled,
    created_at: row.created_at,
  }));
  exportResourceCsv(
    csvRows,
    productExportColumns,
    `products-${exportCommand}-${Date.now().toString(36)}.csv`,
  );
};
const handleProductImport = async (items: Record<string, unknown>[]) => {
  await runResourceCsvImport<ProductImportRow>(
    items,
    async (row) => {
      let reservation: ProductIdReservation | null = null;
      try {
        reservation = await createProductReservation();
        await createProduct({
          reservation_id: reservation.id,
          name: row.name,
          enabled: parseBoolean(row.enabled) ?? true,
        });
        reservation = null;
      } finally {
        if (reservation) {
          void releaseProductReservation(reservation.id).catch((error) => {
            logger.error("Failed to release import reservation:", error);
          });
        }
      }
    },
    {
      onError: (error) => {
        logger.error("Failed to import product:", error);
      },
    },
  );
};
onMounted(() => {
  loadData();
});
watch(createVisible, async (visible) => {
  if (!visible && !reservationConsumed.value) {
    await releaseActiveReservation();
  }
});
onBeforeUnmount(async () => {
  if (!reservationConsumed.value) {
    await releaseActiveReservation();
  }
});
</script>
