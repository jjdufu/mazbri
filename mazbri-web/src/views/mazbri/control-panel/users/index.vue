<template>
  <ControlPanelPageShell
    :title="$t('userManagement.page.title')"
    icon="ri:user-settings-line"
  >
    <ResourceTableShell
      v-model:columns="columns"
      :title="$t('userManagement.list.title')"
      :loading="loading"
      :data="tableData"
      :pagination="artPagination"
      :empty-text="emptyDescription"
      stripe
      @refresh="getData"
      @selection-change="handleSelectionChange"
      @pagination:size-change="handleSizeChange"
      @pagination:current-change="handleCurrentChange"
    >
      <template #actions>
        <ResourceTableActionBar
          v-model="searchParams.q"
          v-model:filters="listFilters"
          :filter-catalog="usersCatalog"
          :show-import="true"
          :import-label="$t('common.actions.import')"
          :show-export="true"
          :export-items="exportItems"
          :export-disabled="tableData.length === 0"
          :show-primary="true"
          :primary-label="$t('userManagement.list.addUser')"
          :show-delete="true"
          :delete-total-count="tableData.length"
          :delete-selected-count="selectedRows.length"
          @search="handleSearch"
          @import="openImport"
          @export="handleExportCommand"
          @primary="handleCreate"
          @delete="(type) => handleDeleteCommand(type, tableData)"
        />
      </template>

      <template #role="{ row }">
        <ElTag :type="getRoleType(row.role)" effect="light" size="small">
          {{ getRoleLabel(row.role) }}
        </ElTag>
      </template>
      <template #status="{ row }">
        <ElTag
          :type="row.is_active !== false ? 'success' : 'danger'"
          effect="light"
          size="small"
        >
          {{
            row.is_active !== false
              ? $t("userManagement.status.active")
              : $t("userManagement.status.inactive")
          }}
        </ElTag>
      </template>
      <template #mfa_enabled="{ row }">
        <ElTag
          :type="row.mfa_enabled ? 'success' : 'info'"
          :effect="row.mfa_enabled ? 'light' : 'plain'"
          size="small"
        >
          {{
            row.mfa_enabled
              ? $t("userManagement.status.mfaBound")
              : $t("userManagement.status.mfaUnbound")
          }}
        </ElTag>
      </template>
      <template #mfa_required="{ row }">
        <ElTag
          :type="row.mfa_required ? 'warning' : 'info'"
          :effect="row.mfa_required ? 'light' : 'plain'"
          size="small"
        >
          {{
            row.mfa_required
              ? $t("userManagement.status.policyOn")
              : $t("userManagement.status.policyOff")
          }}
        </ElTag>
      </template>
      <template #last_login_at="{ row }">
        {{
          row.last_login_at ? formatDate(row.last_login_at) : EMPTY_PLACEHOLDER
        }}
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
            v-if="row.role !== 'R_ADMIN' && row.id !== currentUserId"
            link
            type="danger"
            @click="handleDelete(row)"
          >
            {{ $t("common.actions.delete") }}
          </ElButton>
        </TableActionRow>
      </template>
    </ResourceTableShell>

    <CreateDialog
      v-if="createVisible"
      v-model="createVisible"
      @success="getData"
    />

    <ImportDialog
      v-model="importVisible"
      :title="$t('common.actions.import')"
      :columns="adminImportColumns"
      :format-hint="adminImportFormatHint"
      unique-key="username"
      :existing-data="adminImportExistingData"
      :validator="validateAdminImportRow"
      :transformer="transformAdminImportRow"
      :on-import="handleUserImport"
      :show-success-message="false"
      :show-error-message="false"
      @success="getData"
    />

    <UserDetailDrawer
      v-model="detailVisible"
      :admin-id="selectedAdmin?.id ?? null"
      :initial-admin="selectedAdmin"
      @changed="getData"
      @deleted="getData"
    />
  </ControlPanelPageShell>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage, ElMessageBox } from "element-plus";
import { $t } from "@/locales";
import type {
  AdminUser,
  AdminStatus,
  AdminRole,
  AdminListResponse,
} from "@/types/api/admin";
import type { ListFilter } from "@/types/list-query";
import { usersCatalog } from "@/list-catalog/common-resources";
import { createAdmin, fetchAdminList, deleteAdmin } from "@/api/admin";
import CreateDialog from "./modules/create-dialog.vue";
import UserDetailDrawer from "./modules/UserDetailDrawer.vue";
import ImportDialog from "@/components/common/ImportDialog.vue";
import {
  exportResourceCsv,
  runResourceCsvImport,
} from "@/composables/useResourceCsvImportExport";
import { useTableSelection } from "@/composables/useTableSelection";
import { useTable } from "@/hooks/core/useTable";
import { useUserStore } from "@/store/modules/user";
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import ResourceTableShell from "@/components/common/resource-table-shell/index.vue";
import ResourceTableActionBar from "@/components/common/resource-table-action-bar/index.vue";
import { logger } from "@/utils/sys";
import type { ColumnConfig } from "@/composables/useResourceCsvImportExport";
import { getRoleType, getRoleLabel } from "@/utils/display-labels";
import {
  EMPTY_PLACEHOLDER,
  formatDateTime as formatDate,
} from "@/utils/format";
import type { ColumnOption, PaginationConfig } from "@/types/component";

defineOptions({ name: "UserList" });

type ExportCommand = "page" | "selected";

interface AdminImportRow extends Record<string, unknown> {
  username: string;
  display_name: string;
  email?: string;
  password: string;
  role: AdminRole;
  force_password_reset: boolean;
}

interface AdminExportRow extends Record<string, unknown> {
  username: string;
  display_name: string;
  email: string;
  role: AdminRole;
  status: string;
  mfa_enabled: boolean;
  mfa_required: boolean;
  force_password_reset: boolean;
  created_at: string;
}

const { t, locale } = useI18n();
const userStore = useUserStore();
const currentUserId = computed(() => String(userStore.getUserInfo.id ?? ""));

const getUserStatus = (row: AdminUser): AdminStatus =>
  row.is_active !== false ? "active" : "inactive";

const createVisible = ref(false);
const importVisible = ref(false);
const detailVisible = ref(false);
const selectedAdmin = ref<AdminUser | null>(null);

const table = useTable({
  core: {
    apiFn: fetchAdminList,
    apiParams: { q: "", role: undefined, status: undefined },
    immediate: true,
    paginationKey: { current: "page", size: "page_size" },
  },
  transform: {
    responseAdapter: (res: AdminListResponse) => ({
      records: res.items,
      total: res.total,
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

const searchParams = table.searchParams as {
  q: string;
  role?: AdminRole;
  status?: AdminStatus;
};

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

const buildUserColumns = (): ColumnOption<AdminUser>[] => {
  const isEn = String(locale.value).startsWith("en");
  return [
    { type: "selection", width: 48 },
    {
      prop: "username",
      label: t("userManagement.list.columns.username"),
      minWidth: 108,
    },
    {
      prop: "display_name",
      label: t("userManagement.list.columns.displayName"),
      minWidth: 112,
    },
    {
      prop: "email",
      label: t("userManagement.list.columns.email"),
      minWidth: 132,
    },
    {
      prop: "role",
      label: t("userManagement.list.columns.role"),
      minWidth: isEn ? 168 : 140,
      useSlot: true,
    },
    {
      prop: "status",
      label: t("userManagement.list.columns.status"),
      minWidth: 84,
      align: "center",
      useSlot: true,
    },
    {
      prop: "mfa_enabled",
      label: t("userManagement.list.columns.mfa"),
      minWidth: isEn ? 88 : 80,
      align: "center",
      useSlot: true,
    },
    {
      prop: "mfa_required",
      label: t("userManagement.list.columns.mfaRequired"),
      minWidth: isEn ? 104 : 88,
      align: "center",
      useSlot: true,
    },
    {
      prop: "last_login_at",
      label: t("userManagement.list.columns.lastLogin"),
      minWidth: 180,
      useSlot: true,
    },
    {
      prop: "created_at",
      label: t("userManagement.list.columns.createdAt"),
      minWidth: 180,
      useSlot: true,
    },
    {
      prop: "operations",
      label: t("common.fields.operations"),
      width: isEn ? 156 : 132,
      fixed: "right",
      useSlot: true,
    },
  ];
};

const columns = ref<ColumnOption<AdminUser>[]>(buildUserColumns());
watch(locale, () => {
  const prev = new Map(
    columns.value.map((col) => [String(col.prop ?? col.type ?? ""), col]),
  );
  columns.value = buildUserColumns().map((col) => {
    const existing = prev.get(String(col.prop ?? col.type ?? ""));
    return existing
      ? {
          ...col,
          visible: existing.visible ?? col.visible,
          checked: existing.checked ?? col.checked,
        }
      : col;
  });
});

const adminImportColumns: ColumnConfig[] = [
  {
    key: "username",
    label: $t("userManagement.list.columns.username"),
    required: true,
  },
  {
    key: "display_name",
    label: $t("userManagement.list.columns.displayName"),
    required: true,
  },
  {
    key: "email",
    label: $t("userManagement.list.columns.email"),
    required: false,
  },
  {
    key: "password",
    label: $t("userManagement.createDialog.fields.password"),
    required: true,
  },
  {
    key: "role",
    label: $t("userManagement.list.columns.role"),
    required: true,
  },
  {
    key: "force_password_reset",
    label: $t("userManagement.createDialog.fields.firstLogin"),
    required: false,
  },
];

const adminExportColumns: { key: keyof AdminExportRow; label: string }[] = [
  { key: "username", label: $t("userManagement.list.columns.username") },
  { key: "display_name", label: $t("userManagement.list.columns.displayName") },
  { key: "email", label: $t("userManagement.list.columns.email") },
  { key: "role", label: $t("userManagement.list.columns.role") },
  { key: "status", label: $t("userManagement.list.columns.status") },
  { key: "mfa_enabled", label: $t("userManagement.list.columns.mfa") },
  {
    key: "mfa_required",
    label: $t("userManagement.list.columns.mfaRequired"),
  },
  {
    key: "force_password_reset",
    label: $t("userManagement.createDialog.fields.firstLogin"),
  },
  { key: "created_at", label: $t("userManagement.list.columns.createdAt") },
];

const adminImportExistingData = computed(() =>
  tableData.value.map((row) => ({
    username: row.username,
  })),
);

const adminImportFormatHint = computed(() =>
  [
    $t("feedback.import.csvFormatHint"),
    $t("feedback.import.requiredColumns", {
      columns: adminImportColumns
        .filter((column) => column.required)
        .map((column) => column.key)
        .join(", "),
    }),
  ].join("\n"),
);

const hasActiveSearch = computed(
  () => Boolean(searchParams.q?.trim()) || listFilters.value.length > 0,
);
const emptyDescription = computed(() =>
  hasActiveSearch.value
    ? $t("common.feedback.noMatchingData")
    : $t("common.feedback.noData"),
);

const artPagination = computed<PaginationConfig>(() => ({
  current: pagination.current,
  size: pagination.size,
  total: pagination.total,
}));

const { selectedRows, handleSelectionChange, handleDeleteCommand } =
  useTableSelection<AdminUser>({
    deleteApi: async (row) => {
      if (row.role === "R_ADMIN") {
        throw new Error($t("userManagement.messages.deleteSystemAdminBlocked"));
      }
      await deleteAdmin(row.id);
    },
    getItemName: (row) => row.username,
    onDeleteSuccess: () => getData(),
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

const handleSearch = () => {
  if (pagination.current === 1) {
    getData();
    return;
  }
  void handleCurrentChange(1);
};

const normalizeRole = (value?: string): AdminRole | undefined => {
  const normalized = value?.trim().toUpperCase();
  return normalized === "R_FINANCE" || normalized === "R_VIEWER"
    ? normalized
    : undefined;
};

const normalizeForcePasswordReset = (value?: string) => {
  if (!value?.trim()) {
    return true;
  }
  const normalized = value.trim().toLowerCase();
  return (
    normalized === "true" ||
    normalized === "1" ||
    normalized === "yes" ||
    normalized === "y"
  );
};

const validateAdminImportRow = (row: Record<string, string>) => {
  if (!row.username?.trim()) {
    return {
      valid: false,
      reason: $t("userManagement.createDialog.validation.usernameRequired"),
    };
  }
  if (!row.display_name?.trim()) {
    return {
      valid: false,
      reason: $t("userManagement.createDialog.validation.displayNameRequired"),
    };
  }
  if (!row.password?.trim()) {
    return {
      valid: false,
      reason: $t("userManagement.createDialog.validation.passwordRequired"),
    };
  }
  if (!normalizeRole(row.role)) {
    return {
      valid: false,
      reason: $t("userManagement.createDialog.validation.roleRequired"),
    };
  }
  return { valid: true };
};

const transformAdminImportRow = (
  row: Record<string, string>,
): AdminImportRow => ({
  username: row.username.trim(),
  display_name: row.display_name.trim(),
  email: row.email?.trim() || undefined,
  password: row.password.trim(),
  role: normalizeRole(row.role) as AdminRole,
  force_password_reset: normalizeForcePasswordReset(row.force_password_reset),
});

const openImport = () => {
  importVisible.value = true;
};

const handleExportCommand = (command: string) => {
  const exportCommand = command as ExportCommand;
  const rows =
    exportCommand === "selected" ? selectedRows.value : tableData.value;
  const exportRows: AdminExportRow[] = rows.map((row) => ({
    username: row.username,
    display_name: row.display_name,
    email: row.email || "",
    role: row.role,
    status: getUserStatus(row),
    mfa_enabled: row.mfa_enabled,
    mfa_required: row.mfa_required,
    force_password_reset: row.force_password_reset,
    created_at: row.created_at,
  }));
  exportResourceCsv(
    exportRows,
    adminExportColumns,
    `users-${exportCommand}-${Date.now().toString(36)}.csv`,
  );
};

const handleUserImport = async (items: Record<string, unknown>[]) => {
  await runResourceCsvImport<AdminImportRow>(
    items,
    async (row) => {
      await createAdmin(
        {
          username: row.username,
          display_name: row.display_name,
          email: row.email,
          password: row.password,
          role: row.role,
          force_password_reset: row.force_password_reset,
        },
        { showSuccessMessage: false },
      );
    },
    {
      onError: (error) => {
        logger.error("Failed to import user:", error);
      },
    },
  );
};

const handleCreate = () => {
  createVisible.value = true;
};

const handleView = (row: AdminUser) => {
  selectedAdmin.value = row;
  detailVisible.value = true;
};

const handleDelete = async (row: AdminUser) => {
  if (row.role === "R_ADMIN") {
    void ElMessageBox.alert(
      $t("userManagement.messages.deleteSystemAdminBlocked"),
      $t("common.error"),
      { type: "error" },
    );
    return;
  }
  try {
    await ElMessageBox.confirm(
      $t("userManagement.messages.deleteUserConfirm", {
        username: row.username,
      }),
      $t("common.tips"),
      {
        confirmButtonText: $t("common.confirm"),
        cancelButtonText: $t("common.cancel"),
        type: "warning",
      },
    );
    await deleteAdmin(row.id);
    ElMessage.success($t("common.feedback.deleteSuccess"));
    getData();
  } catch (error) {
    if (error !== "cancel") {
      logger.error("Failed to delete user:", error);
    }
  }
};
</script>
