<template>
  <ControlPanelPageShell
    :title="$t('menus.mazbri.tenantList')"
    icon="ri:building-line"
  >
    <div ref="pageRef" class="tenant-list-page" style="display: flex; flex-direction: column; flex: 1; min-height: 0;">
      <ResourceTableShell
      v-model:columns="columns"
      :title="$t('tenant.list.title')"
      :data="tableData"
      :loading="loading"
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
          :filter-catalog="tenantsCatalog"
          :show-import="true"
          :import-label="$t('common.actions.import')"
          :import-roles="['R_ADMIN', 'R_OPERATOR']"
          :export-items="exportItems"
          :export-disabled="tableData.length === 0"
          :export-roles="['R_ADMIN', 'R_OPERATOR']"
          :show-primary="true"
          :primary-label="$t('tenant.actions.addTenant')"
          :primary-roles="['R_ADMIN', 'R_OPERATOR']"
          :show-delete="true"
          :delete-total-count="tableData.length"
          :delete-selected-count="selectedRows.length"
          :delete-roles="['R_ADMIN']"
          :delete-label="$t('common.actions.delete')"
          @search="handleSearch"
          @import="openImport"
          @export="handleExportCommand"
          @primary="handleCreate"
          @delete="(type) => handleDeleteCommand(type, tableData)"
        />
      </template>

      <template #status="{ row }">
        <StatusIndicator :type="getStatusType(normalizeTenantStatus(row.status))">
          {{ getStatusLabel(normalizeTenantStatus(row.status), tenantStatusLabels) }}
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
            v-if="normalizeTenantStatus(row.status) === 'active'"
            v-roles="['R_ADMIN', 'R_OPERATOR']"
            link
            type="warning"
            @click="handleToggleStatus(row, false)"
          >
            {{ $t("tenant.actions.disable") }}
          </ElButton>
          <ElButton
            v-else
            v-roles="['R_ADMIN', 'R_OPERATOR']"
            link
            type="success"
            @click="handleToggleStatus(row, true)"
          >
            {{ $t("tenant.actions.enable") }}
          </ElButton>
          <ElButton
            v-roles="['R_ADMIN']"
            link
            type="danger"
            @click="handleDelete(row)"
          >
            {{ $t("common.actions.delete") }}
          </ElButton>
        </TableActionRow>
      </template>
    </ResourceTableShell>

    <CreateDialog v-model="createVisible" @success="getData" />
    <ImportDialog
      v-model="importVisible"
      :title="$t('common.actions.import')"
      :columns="tenantImportColumns"
      :format-hint="tenantImportFormatHint"
      unique-key="tenant_code"
      :existing-data="tenantImportExistingData"
      :validator="validateTenantImportRow"
      :transformer="transformTenantImportRow"
      :on-import="handleTenantImport"
      :show-success-message="false"
      :show-error-message="false"
      @success="getData"
    />
    </div>
  </ControlPanelPageShell>
</template>

<script setup lang="ts">
import { ElMessageBox, ElMessage } from "element-plus";
import { $t } from "@/locales";
import type {
  Tenant,
  TenantStatus,
  TenantListResponse,
} from "@/types/api/tenant";
import {
  createTenant,
  fetchTenantList,
  deleteTenant,
  setTenantStatus,
} from "@/api/tenant";
import { useUserStore } from "@/store/modules/user";
import CreateDialog from "./modules/create-dialog.vue";
import ImportDialog from "@/components/common/ImportDialog.vue";
import StatusIndicator from "@/components/common/StatusIndicator.vue";
import ResourceTableShell from "@/components/common/resource-table-shell/index.vue";
import ResourceTableActionBar from "@/components/common/resource-table-action-bar/index.vue";
import { useAdaptiveTableColumns } from "@/composables/useAdaptiveTableColumns";
import {
  exportResourceCsv,
  runResourceCsvImport,
} from "@/composables/useResourceCsvImportExport";
import { useTableSelection } from "@/composables/useTableSelection";
import type { ColumnConfig } from "@/composables/useResourceCsvImportExport";
import { useTable } from "@/hooks/core/useTable";
import { logger } from "@/utils/sys";
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import { getStatusType, getStatusLabel } from "@/utils/display-labels";
import { formatDateTime as formatDate } from "@/utils/format";
import { tenantsCatalog } from "@/list-catalog/tenants";
import type { ListFilter } from "@/types/list-query";
import { watch } from "vue";
import type { ColumnOption } from "@/types/component";
defineOptions({ name: "TenantList" });
type ExportCommand = "page" | "selected";
interface TenantImportRow extends Record<string, unknown> {
  tenant_code: string;
  tenant_id: string;
  name: string;
  contact_person: string;
  contact_phone: string;
  contact_email: string;
}
interface TenantExportRow extends Record<string, unknown> {
  tenant_code: string;
  tenant_id: string;
  name: string;
  contact_person: string;
  contact_phone: string;
  contact_email: string;
  status: string;
  created_at: string;
}
const router = useRouter();
const pageRef = ref<HTMLElement | null>(null);
const createVisible = ref(false);
const importVisible = ref(false);
const columns = ref<ColumnOption<Tenant>[]>([]);
const tenantColumns: ColumnOption<Tenant>[] = [
  { type: "selection", width: 55 },
  { prop: "tenant_code", label: $t("tenant.fields.code"), width: 120 },
  { prop: "tenant_id", label: $t("tenant.fields.accountId"), width: 150 },
  { prop: "name", label: $t("common.fields.name"), minWidth: 100 },
  {
    prop: "contact_person",
    label: $t("tenant.fields.contactPerson"),
    width: 120,
  },
  {
    prop: "contact_phone",
    label: $t("tenant.fields.contactPhone"),
    width: 130,
  },
  {
    prop: "contact_email",
    label: $t("tenant.fields.contactEmail"),
    minWidth: 120,
  },
  {
    prop: "status",
    label: $t("common.fields.status"),
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
    width: 150,
    fixed: "right",
    useSlot: true,
  },
];
const tenantImportColumns: ColumnConfig[] = [
  { key: "tenant_code", label: $t("tenant.fields.code"), required: true },
  { key: "tenant_id", label: $t("tenant.fields.accountId"), required: true },
  { key: "name", label: $t("common.fields.name"), required: true },
  {
    key: "contact_person",
    label: $t("tenant.fields.contactPerson"),
    required: true,
  },
  {
    key: "contact_phone",
    label: $t("tenant.fields.contactPhone"),
    required: true,
  },
  {
    key: "contact_email",
    label: $t("tenant.fields.contactEmail"),
    required: true,
  },
];
const tenantExportColumns: { key: keyof TenantExportRow; label: string }[] = [
  { key: "tenant_code", label: $t("tenant.fields.code") },
  { key: "tenant_id", label: $t("tenant.fields.accountId") },
  { key: "name", label: $t("common.fields.name") },
  { key: "contact_person", label: $t("tenant.fields.contactPerson") },
  { key: "contact_phone", label: $t("tenant.fields.contactPhone") },
  { key: "contact_email", label: $t("tenant.fields.contactEmail") },
  { key: "status", label: $t("common.fields.status") },
  { key: "created_at", label: $t("common.fields.createdAt") },
];
const table = useTable({
  core: {
    apiFn: fetchTenantList,
    apiParams: { q: "", status: undefined },
    immediate: true,
    paginationKey: { current: "page", size: "page_size" },
  },
  transform: {
    responseAdapter: (res: TenantListResponse) => ({
      records: res.tenants,
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
  resetSearchParams,
} = table;
const searchParams = table.searchParams!;
const listFilters = ref<ListFilter[]>([]);
const searchKeyword = computed<string>({
  get: () => (searchParams as { q?: string }).q ?? "",
  set: (value) => {
    (searchParams as { q?: string }).q = value;
  },
});

watch(
  listFilters,
  (filters) => {
    const params = searchParams as { filters?: string };
    if (filters.length) {
      params.filters = JSON.stringify(filters);
    } else {
      delete params.filters;
    }
  },
  { deep: true },
);
const hasActiveSearch = computed(
  () =>
    Boolean((searchParams as { q?: string }).q?.trim()) ||
    listFilters.value.length > 0,
);
const emptyDescription = computed(() =>
  hasActiveSearch.value
    ? $t("common.feedback.noMatchingData")
    : $t("common.feedback.noData"),
);
const getTenantCellText = (columnKey: string, row: Tenant) => {
  switch (columnKey) {
    case "tenant_code":
      return row.tenant_code || "-";
    case "tenant_id":
      return row.tenant_id || "-";
    case "name":
      return row.name || "-";
    case "contact_person":
      return row.contact_person || "-";
    case "contact_phone":
      return row.contact_phone || "-";
    case "contact_email":
      return row.contact_email || "-";
    case "status":
      return getStatusLabel(row.status, tenantStatusLabels.value);
    case "created_at":
      return formatDate(row.created_at);
    case "operations":
      return `${$t("common.actions.details")} ${$t("common.actions.delete")}`;
    default:
      return row.name || row.tenant_code || "-";
  }
};
const getTenantColumnWeight = (columnKey: string) => {
  switch (columnKey) {
    case "tenant_code":
    case "tenant_id":
      return 1.1;
    case "name":
      return 1.4;
    case "contact_person":
    case "contact_phone":
      return 1;
    case "contact_email":
      return 1.8;
    case "status":
      return 0.9;
    case "created_at":
      return 1;
    default:
      return 1;
  }
};
const userStore = useUserStore();

function normalizeTenantStatus(status?: string): TenantStatus {
  if (status === "active") return "active";
  if (status === "pending") return "pending";
  return "suspended";
}
const tenantStatusLabels = computed(() => ({
  active: $t("common.status.active"),
  suspended: $t("common.status.suspended"),
  pending: $t("common.status.pending"),
}));
useAdaptiveTableColumns({
  columns,
  containerRef: pageRef,
  rows: tableData,
  baseColumns: tenantColumns,
  getCellText: ({ columnKey, row }) => getTenantCellText(columnKey, row),
  getColumnWeight: ({ columnKey }) => getTenantColumnWeight(columnKey),
});
const tenantImportRows = ref<Tenant[]>([]);
const tenantImportExistingData = computed(() =>
  tenantImportRows.value.map((row) => ({
    tenant_code: row.tenant_code,
  })),
);
const tenantImportFormatHint = computed(() =>
  [
    $t("feedback.import.csvFormatHint"),
    $t("feedback.import.requiredColumns", {
      columns: tenantImportColumns
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
const { selectedRows, handleSelectionChange, handleDeleteCommand } =
  useTableSelection<Tenant>({
    deleteApi: async (row) => {
      if (!userStore.isAdmin) {
        throw new Error("forbidden");
      }
      await deleteTenant(row.tenant_code);
    },
    getItemName: (row) => row.name,
    onDeleteSuccess: () => getData(),
    allDeleteConfirmMsg: (count) =>
      $t("feedback.batch.confirmAllAction", {
        action: $t("common.actions.delete"),
        count,
      }) +
      " " +
      $t("tenant.messages.deleteTenantConfirm", { name: `×${count}` }),
    selectedDeleteConfirmMsg: (count) =>
      $t("feedback.batch.confirmSelectedAction", {
        action: $t("common.actions.delete"),
        count,
      }),
  });
const handleSearch = () => {
  if (pagination.current === 1) {
    getData();
    return;
  }
  void handleCurrentChange(1);
};

const validateTenantImportRow = (row: Record<string, string>) => {
  if (!row.tenant_code?.trim()) {
    return {
      valid: false,
      reason: $t("tenant.createDialog.validation.codeRequired"),
    };
  }
  if (!row.tenant_id?.trim()) {
    return {
      valid: false,
      reason: $t("tenant.createDialog.validation.accountIdRequired"),
    };
  }
  if (!row.name?.trim()) {
    return {
      valid: false,
      reason: $t("tenant.createDialog.validation.nameRequired"),
    };
  }
  if (!row.contact_person?.trim()) {
    return {
      valid: false,
      reason: $t("tenant.createDialog.validation.contactPersonRequired"),
    };
  }
  if (!row.contact_phone?.trim()) {
    return {
      valid: false,
      reason: $t("tenant.createDialog.validation.contactPhoneRequired"),
    };
  }
  if (!row.contact_email?.trim()) {
    return {
      valid: false,
      reason: $t("tenant.createDialog.validation.contactEmailRequired"),
    };
  }
  return { valid: true };
};
const transformTenantImportRow = (
  row: Record<string, string>,
): TenantImportRow => ({
  tenant_code: row.tenant_code.trim().toUpperCase(),
  tenant_id: row.tenant_id.trim().toLowerCase(),
  name: row.name.trim(),
  contact_person: row.contact_person.trim(),
  contact_phone: row.contact_phone.trim(),
  contact_email: row.contact_email.trim(),
});
const loadTenantImportRows = async () => {
  try {
    const res = await fetchTenantList({ page_size: 1000 });
    tenantImportRows.value = res.tenants;
  } catch {
    tenantImportRows.value = tableData.value;
  }
};
const openImport = async () => {
  await loadTenantImportRows();
  importVisible.value = true;
};
const handleExportCommand = (command: string) => {
  const exportCommand = command as ExportCommand;
  const rows =
    exportCommand === "selected" ? selectedRows.value : tableData.value;
  const exportRows: TenantExportRow[] = rows.map((row) => ({
    tenant_code: row.tenant_code,
    tenant_id: row.tenant_id,
    name: row.name,
    contact_person: row.contact_person || "",
    contact_phone: row.contact_phone || "",
    contact_email: row.contact_email || "",
    status: row.status,
    created_at: row.created_at,
  }));
  exportResourceCsv(
    exportRows,
    tenantExportColumns,
    `tenants-${exportCommand}-${Date.now().toString(36)}.csv`,
  );
};
const handleTenantImport = async (items: Record<string, unknown>[]) => {
  await runResourceCsvImport<TenantImportRow>(
    items,
    async (row) => {
      await createTenant(
        {
          tenant_code: row.tenant_code,
          tenant_id: row.tenant_id,
          name: row.name,
          contact_person: row.contact_person,
          contact_phone: row.contact_phone,
          contact_email: row.contact_email,
        },
        undefined,
        { showSuccessMessage: false },
      );
    },
    {
      onError: (error) => {
        logger.error("Failed to import tenant:", error);
      },
    },
  );
};
const handleReset = () => {
  resetSearchParams();
  handleSearch();
};
const handleCreate = () => {
  createVisible.value = true;
};
const handleView = (row: Tenant) => {
  router.push({ name: "TenantDetail", params: { code: row.tenant_code } });
};
const handleToggleStatus = async (row: Tenant, enable: boolean) => {
  try {
    await ElMessageBox.confirm(
      enable
        ? $t("tenant.messages.enableTenantConfirm", { name: row.name })
        : $t("tenant.messages.disableTenantConfirm", { name: row.name }),
      $t("common.tips"),
      {
        confirmButtonText: $t("common.confirm"),
        cancelButtonText: $t("common.cancel"),
        type: "warning",
      },
    );
    await setTenantStatus(row.tenant_code, {
      status: enable ? "active" : "suspended",
    });
    ElMessage.success(
      enable ? $t("common.feedback.enabled") : $t("common.feedback.disabled"),
    );
    getData();
  } catch (error) {
    if (error !== "cancel") {
      logger.error("Failed to toggle tenant status");
    }
  }
};

const handleDelete = async (row: Tenant) => {
  if (!userStore.isAdmin) {
    ElMessage.error($t("common.feedback.unknownError"));
    return;
  }
  try {
    await ElMessageBox.confirm(
      $t("tenant.messages.deleteTenantConfirm", { name: row.name }),
      $t("common.tips"),
      {
        confirmButtonText: $t("common.confirm"),
        cancelButtonText: $t("common.cancel"),
        type: "warning",
      },
    );
    await deleteTenant(row.tenant_code);
    ElMessage.success($t("common.feedback.deleteSuccess"));
    getData();
  } catch (error) {
    if (error !== "cancel") {
      logger.error("Failed to delete tenant");
    }
  }
};
</script>
