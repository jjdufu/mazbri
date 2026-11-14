<template>
  <ControlPanelPageShell
    :title="$t('configCenter.page.title')"
    icon="ri:settings-3-line"
  >
    <ResourceTableShell
      v-model:columns="columns"
      :loading="loading"
      :data="tableData"
      :pagination="pagination"
      stripe
      table-class="surface-panel-table"
      @refresh="refresh.refresh()"
      @pagination:size-change="refresh.handleSizeChange"
      @pagination:current-change="refresh.handleCurrentChange"
    >
      <template #actions>
        <ResourceTableActionBar
          v-model="searchKeyword"
          v-model:filters="listFilters"
          :filter-catalog="configCenterCatalog"
          :show-import="true"
          :import-label="$t('configCenter.actions.import')"
          :import-disabled="authorizationDisabled"
          :import-disabled-reason="authorizationDisabledReason"
          :show-export="true"
          :export-items="exportItems"
          :export-label="$t('configCenter.actions.export')"
          :export-disabled="authorizationDisabled || tableData.length === 0"
          :export-disabled-reason="
            authorizationDisabled ? authorizationDisabledReason : undefined
          "
          :show-primary="true"
          :primary-label="$t('configCenter.actions.create')"
          :primary-disabled="authorizationDisabled"
          :primary-disabled-reason="authorizationDisabledReason"
          @search="refresh.handleSearch"
          @import="handleImport"
          @export="handleExportCommand"
          @primary="handleCreate"
        />
      </template>

      <template #service_name="{ row }">
        <span class="config-service-name">{{ row.service_name }}</span>
      </template>

      <template #updated_by="{ row }">
        <span>{{ row.updated_by || "—" }}</span>
      </template>

      <template #updated_at="{ row }">
        <span>{{ formatDate(row.updated_at) }}</span>
      </template>

      <template #operation="{ row }">
        <CommonInlineRow gap="8px">
          <AuthorizationTooltipButton
            link
            type="primary"
            :disabled="authorizationDisabled"
            :disabled-reason="authorizationDisabledReason"
            @click="handleEdit(row)"
          >
            {{ $t("configCenter.actions.edit") }}
          </AuthorizationTooltipButton>
          <ElButton link type="primary" @click="handleHistory(row)">
            {{ $t("configCenter.actions.history") }}
          </ElButton>
          <AuthorizationTooltipButton
            link
            type="danger"
            :disabled="
              authorizationDisabled || isCoreProtectedService(row.service_name)
            "
            :disabled-reason="
              isCoreProtectedService(row.service_name)
                ? $t('configCenter.messages.coreProtectedDeleteTip')
                : authorizationDisabledReason
            "
            @click="handleDelete(row)"
          >
            {{ $t("common.actions.delete") }}
          </AuthorizationTooltipButton>
        </CommonInlineRow>
      </template>
    </ResourceTableShell>

    <ConfigEditDialog
      v-model="editVisible"
      :config="currentConfig"
      :existing-names="existingServiceNames"
      :authorization-disabled="authorizationDisabled"
      :authorization-disabled-reason="authorizationDisabledReason"
      @success="handleDialogSuccess('edit')"
    />

    <HistoryDrawer
      v-model="historyVisible"
      :config="currentConfig"
      :authorization-disabled="authorizationDisabled"
      :authorization-disabled-reason="authorizationDisabledReason"
      @rollback="handleDialogSuccess('rollback')"
    />

    <ImportDialog
      v-model="importVisible"
      :authorization-disabled="authorizationDisabled"
      :authorization-disabled-reason="authorizationDisabledReason"
      @success="handleDialogSuccess('import')"
    />

    <ExportDialog
      v-model="exportVisible"
      :authorization-disabled="authorizationDisabled"
      :authorization-disabled-reason="authorizationDisabledReason"
      @success="handleDialogSuccess('export')"
    />
  </ControlPanelPageShell>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import type { ListFilter } from "@/types/list-query";
import { configCenterCatalog } from "@/list-catalog/common-resources";
import { $t } from "@/locales";
import type { ConfigSummary } from "@/types/api/config-center";
import { deleteConfig } from "@/api/config-center";
import AuthorizationTooltipButton from "@/components/common/authorization-tooltip-button/index.vue";
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import ConfigEditDialog from "./modules/ConfigEditDialog.vue";
import HistoryDrawer from "./modules/HistoryDrawer.vue";
import ImportDialog from "./modules/ImportDialog.vue";
import ExportDialog from "./modules/ExportDialog.vue";
import ResourceTableShell from "@/components/common/resource-table-shell/index.vue";
import CommonInlineRow from "@/components/common/inline-row/index.vue";
import ResourceTableActionBar from "@/components/common/resource-table-action-bar/index.vue";
import {
  type ConfigCenterDialogSuccess,
  useConfigCenterDialogs,
} from "./composables/useConfigCenterDialogs";
import { useConfigCenterPage } from "./composables/useConfigCenterPage";
import { useConfigCenterRefresh } from "./composables/useConfigCenterRefresh";
import { useLicenseGate } from "@/composables/useLicenseGate";

defineOptions({ name: "ConfigCenter" });

const {
  columns,
  loading,
  tableData,
  currentConfig,
  searchForm,
  pagination,
  existingServiceNames,
  formatDate,
  loadAllExistingNames,
  loadCoreProtectedNames,
  isCoreProtectedService,
  handleDeleteRow,
} = useConfigCenterPage();

const {
  authorizationDisabled,
  authorizationDisabledReason,
  loadLicenseStatus,
} = useLicenseGate();

const refresh = useConfigCenterRefresh({
  loading,
  tableData,
  searchForm,
  pagination,
});

const {
  editVisible,
  historyVisible,
  importVisible,
  exportVisible,
  handleCreate,
  handleEdit,
  handleHistory,
  handleImport,
  handleExport,
  handleDialogSuccess,
} = useConfigCenterDialogs({
  currentConfig,
  onSuccess: async (kind: ConfigCenterDialogSuccess) => {
    if (kind === "edit") {
      await Promise.all([
        loadAllExistingNames(),
        refresh.refresh({
          resetSearch: true,
          resetPage: true,
        }),
      ]);
    }
    if (kind === "rollback" || kind === "import") {
      await Promise.all([loadAllExistingNames(), refresh.refresh()]);
    }
  },
});

const exportItems = computed(() => [
  { command: "export", label: $t("configCenter.actions.export") },
]);

const listFilters = ref<ListFilter[]>([]);
const searchKeyword = computed<string>({
  get: () => searchForm.q ?? "",
  set: (value: string) => {
    searchForm.q = value || undefined;
  },
});

watch(
  listFilters,
  (filters) => {
    if (filters.length) {
      searchForm.filters = JSON.stringify(filters);
    } else {
      searchForm.filters = undefined;
    }
  },
  { deep: true },
);

const handleExportCommand = (_command: string) => {
  handleExport();
};

const handleDelete = async (row: ConfigSummary) => {
  await handleDeleteRow(
    row,
    (item) => deleteConfig(item.service_name),
    async () => {
      await Promise.all([loadAllExistingNames(), refresh.refresh()]);
    },
  );
};

onMounted(() => {
  void loadLicenseStatus();
  void loadCoreProtectedNames();
  void loadAllExistingNames();
  void refresh.initialize();
});
</script>

<style scoped lang="scss">
.config-service-name {
  font-weight: 500;
  color: var(--el-text-color-primary);
}
</style>
