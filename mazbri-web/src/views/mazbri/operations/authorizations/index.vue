<template>
  <div ref="pageRef" class="authorization-list-page" style="display: flex; flex-direction: column; flex: 1; min-height: 0;">
    <ResourceTableShell
      v-model:columns="columns"
      :title="$t('authorization.list.title')"
      :loading="loading"
      :data="tableData"
      :empty-text="emptyDescription"
      :pagination="pagination"
      :fit="true"
      table-layout="auto"
      @refresh="getData"
      @selection-change="handleSelectionChange"
      @pagination:size-change="handleSizeChange"
      @pagination:current-change="handleCurrentChange"
    >
      <template #actions>
        <ResourceTableActionBar
          v-model="searchKeyword"
          v-model:filters="listFilters"
          :filter-catalog="authorizationsCatalog"
          :show-import="true"
          :import-label="$t('common.actions.import')"
          :import-roles="['R_ADMIN', 'R_OPERATOR']"
          :export-items="exportItems"
          :export-disabled="tableData.length === 0"
          :export-roles="['R_ADMIN', 'R_OPERATOR']"
          :show-primary="true"
          :primary-label="$t('authorization.list.actions.create')"
          :primary-roles="['R_ADMIN', 'R_OPERATOR']"
          :show-delete="true"
          :delete-total-count="tableData.length"
          :delete-selected-count="selectedRows.length"
          :delete-label="$t('common.actions.delete')"
          :delete-roles="['R_ADMIN', 'R_OPERATOR']"
          @search="handleSearch"
          @import="openImport"
          @export="handleExportCommand"
          @primary="createVisible = true"
          @delete="(cmd) => handleDeleteCommand(cmd, tableData)"
        />
      </template>

      <template #unit_price="{ row }">
        {{ row.unit_price }} {{ $t("common.currencies.cny") }}
      </template>
      <template #created_at="{ row }">
        {{ formatDate(row.created_at) }}
      </template>
      <template #operations="{ row }">
        <TableActionRow>
          <ElButton
            v-roles="['R_ADMIN', 'R_OPERATOR']"
            link
            type="primary"
            @click="handleConfigure(row)"
          >
            {{ $t("authorization.list.actions.modify") }}
          </ElButton>
          <ElPopconfirm
            v-roles="['R_ADMIN', 'R_OPERATOR']"
            :title="$t('authorization.list.revokeConfirm')"
            @confirm="handleRevoke(row)"
          >
            <template #reference>
              <ElButton type="danger" link>{{
                $t("authorization.list.actions.revoke")
              }}</ElButton>
            </template>
          </ElPopconfirm>
        </TableActionRow>
      </template>
    </ResourceTableShell>

    <CreateAuthorizationDialog v-model="createVisible" @success="getData" />
    <ImportDialog
      v-model="importVisible"
      :title="$t('common.actions.import')"
      :columns="authorizationImportColumns"
      :format-hint="authorizationImportFormatHint"
      unique-key="unique_key"
      :existing-data="authorizationImportExistingData"
      :validator="validateAuthorizationImportRow"
      :transformer="transformAuthorizationImportRow"
      :on-import="handleAuthorizationImport"
      :show-success-message="false"
      :show-error-message="false"
      @success="getData"
    />
    <ChannelConfigDrawer
      v-model="configDrawerVisible"
      :tenant-code="selectedAuthorization?.tenant_code || ''"
      :product="selectedProduct"
      @updated="getData"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from "vue";
import { $t } from "@/locales";
import { fetchAuthorizations, fetchProductList } from "@/api/product";
import { authorizationsCatalog } from "@/list-catalog/common-resources";
import type { ListFilter } from "@/types/list-query";
import { grantProduct, revokeProduct, fetchTenantList } from "@/api/tenant";
import type {
  AuthorizationDetail,
  AuthorizationListResponse,
} from "@/types/api/product";
import type { Product } from "@/types/api/product";
import type { Tenant, TenantProduct } from "@/types/api/tenant";
import ImportDialog from "@/components/common/ImportDialog.vue";
import ResourceTableShell from "@/components/common/resource-table-shell/index.vue";
import ResourceTableActionBar from "@/components/common/resource-table-action-bar/index.vue";
import { useAdaptiveTableColumns } from "@/composables/useAdaptiveTableColumns";
import {
  exportResourceCsv,
  runResourceCsvImport,
} from "@/composables/useResourceCsvImportExport";
import { useTableSelection } from "@/composables/useTableSelection";
import { useTable } from "@/hooks/core/useTable";
import CreateAuthorizationDialog from "./modules/CreateAuthorizationDialog.vue";
import ChannelConfigDrawer from "@/components/common/channel-config-dialog/index.vue";
import { ElMessage } from "element-plus";
import { logger } from "@/utils/sys";
import type { ColumnConfig } from "@/composables/useResourceCsvImportExport";
import type { ColumnOption } from "@/types/component";
import { formatDateTime as formatDate } from "@/utils/format";
import { createDefaultCBProPolicyConfig } from "@/api/tenant";
defineOptions({ name: "AuthorizationList" });
const pageRef = ref<HTMLElement | null>(null);
const createVisible = ref(false);
const configDrawerVisible = ref(false);
const selectedAuthorization = ref<AuthorizationDetail | null>(null);
const selectedProduct = ref<TenantProduct | null>(null);
const columns = ref<ColumnOption<AuthorizationDetail>[]>([]);
const authorizationColumns: ColumnOption<AuthorizationDetail>[] = [
  { type: "selection", width: 50, align: "center" },
  {
    prop: "tenant_code",
    label: $t("authorization.list.tenantCode"),
    width: 120,
  },
  {
    prop: "tenant_name",
    label: $t("authorization.list.tenantName"),
    minWidth: 100,
    showOverflowTooltip: true,
  },
  {
    prop: "product_code",
    label: $t("authorization.list.productCode"),
    width: 120,
  },
  {
    prop: "product_name",
    label: $t("authorization.list.productName"),
    minWidth: 100,
    showOverflowTooltip: true,
  },
  {
    prop: "unit_price",
    label: $t("authorization.list.unitPrice"),
    width: 100,
    useSlot: true,
  },
  {
    prop: "created_at",
    label: $t("authorization.list.createdAt"),
    width: 170,
    useSlot: true,
  },
  {
    prop: "operations",
    label: $t("common.fields.operations"),
    width: 160,
    fixed: "right",
    useSlot: true,
  },
];
const table = useTable({
  core: {
    apiFn: fetchAuthorizations,
    apiParams: { q: "" },
    immediate: true,
    paginationKey: { current: "page", size: "page_size" },
  },
  transform: {
    responseAdapter: (res: AuthorizationListResponse) => ({
      records: res.authorizations,
      total: res.pagination?.total ?? 0,
    }),
  },
});
const {
  data: tableData,
  loading,
  pagination,
  getData,
  handleSizeChange,
  handleCurrentChange,
} = table;
const searchParams = table.searchParams!;
const listFilters = ref<ListFilter[]>([]);
watch(
  listFilters,
  (filters) => {
    const params = searchParams as { filters?: string };
    if (filters.length) params.filters = JSON.stringify(filters);
    else delete params.filters;
  },
  { deep: true },
);
const searchKeyword = computed<string>({
  get: () => searchParams.q ?? "",
  set: (value) => {
    searchParams.q = value;
  },
});
const { selectedRows, handleSelectionChange, handleDeleteCommand } =
  useTableSelection<AuthorizationDetail>({
    deleteApi: (auth) => revokeProduct(auth.tenant_code, auth.product_code),
    getItemName: (auth) => `${auth.tenant_name} - ${auth.product_name}`,
    onDeleteSuccess: getData,
    actionLabel: $t("authorization.list.actions.batchRevoke"),
  });
interface AuthorizationImportRow extends Record<string, unknown> {
  unique_key: string;
  tenant_code: string;
  product_code: string;
  unit_price: string;
  cb_pro_fallback: boolean;
}
interface AuthorizationExportRow extends Record<string, unknown> {
  tenant_code: string;
  tenant_name: string;
  product_code: string;
  product_name: string;
  unit_price: string;
  cb_pro_fallback: boolean;
  enabled: boolean;
  created_at: string;
}
const importVisible = ref(false);
const importTenants = ref<Tenant[]>([]);
const importProducts = ref<Product[]>([]);
const authorizationImportColumns: ColumnConfig[] = [
  {
    key: "tenant_code",
    label: $t("authorization.list.tenantCode"),
    required: true,
  },
  {
    key: "product_code",
    label: $t("authorization.list.productCode"),
    required: true,
  },
  {
    key: "unit_price",
    label: $t("authorization.list.unitPrice"),
    required: false,
  },
  {
    key: "cb_pro_fallback",
    label: $t("authorization.config.cbProFallback"),
    required: false,
  },
];
const authorizationImportExistingData = computed(() =>
  tableData.value.map((auth) => ({
    unique_key: `${auth.tenant_code}/${auth.product_code}`,
  })),
);
const authorizationImportFormatHint = computed(() =>
  [
    $t("feedback.import.csvFormatHint"),
    $t("feedback.import.requiredColumns", {
      columns: authorizationImportColumns
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
  if (["true", "1", "yes", "y", "on", "enabled"].includes(normalized)) {
    return true;
  }
  if (["false", "0", "no", "n", "off", "disabled"].includes(normalized)) {
    return false;
  }
  return undefined;
};
const parseUnitPrice = (value?: string) => {
  if (!value?.trim()) {
    return "0.001";
  }
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed > 0 ? value.trim() : "";
};
const loadImportOptions = async () => {
  try {
    const [tenantRes, productRes] = await Promise.all([
      fetchTenantList({ page_size: 1000 }),
      fetchProductList({ page_size: 1000 }),
    ]);
    importTenants.value = tenantRes.tenants;
    importProducts.value = productRes.products;
  } catch (error) {
    logger.error("Failed to load authorization import options:", error);
    importTenants.value = [];
    importProducts.value = [];
  }
};
const hasActiveSearch = computed(
  () => Boolean(searchParams.q?.trim()) || listFilters.value.length > 0,
);
const emptyDescription = computed(() =>
  hasActiveSearch.value
    ? $t("common.feedback.noMatchingData")
    : $t("common.feedback.noData"),
);
const getAuthorizationCellText = (
  columnKey: string,
  row: AuthorizationDetail,
) => {
  switch (columnKey) {
    case "tenant_code":
      return row.tenant_code || "-";
    case "tenant_name":
      return row.tenant_name || "-";
    case "product_code":
      return row.product_code || "-";
    case "product_name":
      return row.product_name || "-";
    case "unit_price":
      return `${row.unit_price} ${$t("common.currencies.cny")}`;
    case "created_at":
      return formatDate(row.created_at);
    case "operations":
      return `${$t("authorization.list.actions.modify")} ${$t("authorization.list.actions.revoke")}`;
    default:
      return row.tenant_name || row.product_name || "-";
  }
};
const getAuthorizationColumnWeight = (columnKey: string) => {
  switch (columnKey) {
    case "tenant_code":
    case "product_code":
      return 1;
    case "tenant_name":
    case "product_name":
      return 1.8;
    case "unit_price":
      return 0.9;
    case "created_at":
      return 1.1;
    default:
      return 1;
  }
};
useAdaptiveTableColumns({
  columns,
  containerRef: pageRef,
  rows: tableData,
  baseColumns: authorizationColumns,
  selectionWidth: 50,
  operationsWidth: 160,
  getCellText: ({ columnKey, row }) => getAuthorizationCellText(columnKey, row),
  getColumnWeight: ({ columnKey }) => getAuthorizationColumnWeight(columnKey),
});
const buildTenantProductFromAuthorization = (
  auth: AuthorizationDetail,
): TenantProduct => ({
  product_id: auth.product_code,
  name: auth.product_name,
  description: "",
  cb_pro_enabled: auth.cb_pro_enabled,
  config: {
    unit_price: auth.unit_price,
    currency: auth.currency,
    cb_pro_fallback: auth.cb_pro_fallback,
    request_codec: auth.config?.request_codec ?? "json",
    response_codec: auth.config?.response_codec ?? "json",
    codec_enabled: auth.config?.codec_enabled ?? false,
    codec_version: auth.config?.codec_version ?? 1,
    cbpro: auth.config?.cbpro ?? createDefaultCBProPolicyConfig(),
  },
  granted_at: auth.created_at,
  updated_at: auth.updated_at,
});
const handleConfigure = (row: AuthorizationDetail) => {
  selectedAuthorization.value = row;
  selectedProduct.value = buildTenantProductFromAuthorization(row);
  configDrawerVisible.value = true;
};
const handleRevoke = async (row: AuthorizationDetail) => {
  try {
    await revokeProduct(row.tenant_code, row.product_code);
    ElMessage.success($t("authorization.list.revokeSuccess"));
    getData();
  } catch (error) {
    logger.error("Failed to revoke authorization:", error);
  }
};
const validateAuthorizationImportRow = (row: Record<string, string>) => {
  if (!row.tenant_code?.trim()) {
    return {
      valid: false,
      reason: $t("authorization.grantDialog.selectTenant"),
    };
  }
  if (!row.product_code?.trim()) {
    return {
      valid: false,
      reason: $t("authorization.grantDialog.selectProduct"),
    };
  }
  const tenantExists = importTenants.value.some(
    (tenant) => tenant.tenant_code === row.tenant_code.trim(),
  );
  if (!tenantExists) {
    return {
      valid: false,
      reason: `${$t("authorization.list.tenantCode")}: ${row.tenant_code}`,
    };
  }
  const productExists = importProducts.value.some(
    (product) => product.product_id === row.product_code.trim(),
  );
  if (!productExists) {
    return {
      valid: false,
      reason: `${$t("authorization.list.productCode")}: ${row.product_code}`,
    };
  }
  const unitPrice = parseUnitPrice(row.unit_price);
  if (!unitPrice) {
    return { valid: false, reason: $t("authorization.list.unitPrice") };
  }
  const fallback = row.cb_pro_fallback?.trim();
  if (fallback && parseBoolean(fallback) === undefined) {
    return {
      valid: false,
      reason: `${$t("authorization.config.cbProFallback")}: ${fallback}`,
    };
  }
  return { valid: true };
};
const transformAuthorizationImportRow = (
  row: Record<string, string>,
): AuthorizationImportRow => ({
  unique_key: `${row.tenant_code.trim()}/${row.product_code.trim()}`,
  tenant_code: row.tenant_code.trim(),
  product_code: row.product_code.trim(),
  unit_price: parseUnitPrice(row.unit_price) || "0.001",
  cb_pro_fallback: parseBoolean(row.cb_pro_fallback) ?? false,
});
const handleExportCommand = (command: string) => {
  const exportCommand = command as "page" | "selected";
  if (exportCommand !== "page" && exportCommand !== "selected") {
    return;
  }
  const rows = exportCommand === "page" ? tableData.value : selectedRows.value;
  const csvRows: AuthorizationExportRow[] = rows.map((row) => ({
    tenant_code: row.tenant_code,
    tenant_name: row.tenant_name,
    product_code: row.product_code,
    product_name: row.product_name,
    unit_price: row.unit_price,
    cb_pro_fallback: row.cb_pro_fallback,
    enabled: row.enabled,
    created_at: row.created_at,
  }));
  exportResourceCsv(
    csvRows,
    [
      { key: "tenant_code", label: $t("authorization.list.tenantCode") },
      { key: "tenant_name", label: $t("authorization.list.tenantName") },
      { key: "product_code", label: $t("authorization.list.productCode") },
      { key: "product_name", label: $t("authorization.list.productName") },
      { key: "unit_price", label: $t("authorization.list.unitPrice") },
      {
        key: "cb_pro_fallback",
        label: $t("authorization.config.cbProFallback"),
      },
      { key: "enabled", label: $t("common.fields.status") },
      { key: "created_at", label: $t("authorization.list.createdAt") },
    ],
    `authorizations-${exportCommand}-${Date.now().toString(36)}.csv`,
  );
};
const openImport = async () => {
  await loadImportOptions();
  importVisible.value = true;
};
const handleAuthorizationImport = async (items: Record<string, unknown>[]) => {
  await runResourceCsvImport<AuthorizationImportRow>(
    items,
    async (row) => {
      const product = importProducts.value.find(
        (candidate) => candidate.product_id === row.product_code,
      );
      await grantProduct(
        row.tenant_code,
        row.product_code,
        {
          unit_price: row.unit_price,
          cb_pro_fallback: product?.cb_pro_enabled
            ? row.cb_pro_fallback
            : false,
        },
        {
          showSuccessMessage: false,
        },
      );
    },
    {
      onError: (error) => {
        logger.error("Failed to import authorization:", error);
      },
    },
  );
};
const handleSearch = () => {
  if (pagination.current === 1) {
    getData();
    return;
  }
  void handleCurrentChange(1);
};

</script>

<style scoped lang="scss">
.authorization-list-page {
  padding: 0;
}
</style>
