<template>
  <ControlPanelPageShell
    :title="$t('configCenter.page.title')"
    icon="ri:settings-3-line"
  >
    <NodeStatusCard />

    <div class="toolbar">
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
        :show-delete="true"
        :delete-total-count="tableData.length"
        :delete-selected-count="selectedRows.length"
        :delete-label="$t('common.actions.delete')"
        :delete-disabled="authorizationDisabled"
        :delete-disabled-reason="authorizationDisabledReason"
        @search="refresh.handleSearch"
        @import="handleImport"
        @export="handleExportCommand"
        @primary="handleCreate"
        @delete="handleDeleteAction"
      />
    </div>

    <ResourceTableShell
      :columns="[]"
      :loading="loading"
      :data="tableData"
      :pagination="pagination"
      :show-header="false"
      table-class="surface-panel-table"
      show-toolbar
      toolbar-layout="size,fullscreen,columns,settings"
      @selection-change="handleTableSelectionChange"
      @pagination:size-change="refresh.handleSizeChange"
      @pagination:current-change="refresh.handleCurrentChange"
    >
      <template #default>
        <ElTableColumn type="selection" width="55" />
        <ElTableColumn
          prop="service_name"
          :label="$t('configCenter.columns.serviceName')"
          min-width="250"
        >
          <template #default="{ row }">
            <div class="service-name-cell" :data-copy-text="row.service_name">
              <ArtSvgIcon
                icon="ri:server-line"
                class="service-icon text-theme"
              />
              <div class="service-info">
                <span class="service-name">{{ row.service_name }}</span>
              </div>
            </div>
          </template>
        </ElTableColumn>
        <ElTableColumn
          prop="updated_by"
          :label="$t('configCenter.columns.updatedBy')"
          min-width="150"
        >
          <template #default="{ row }">
            <div class="updated-by-cell">
              <ArtSvgIcon icon="ri:user-line" class="user-icon text-g-500" />
              <span>{{ row.updated_by || "-" }}</span>
            </div>
          </template>
        </ElTableColumn>
        <ElTableColumn
          prop="updated_at"
          :label="$t('configCenter.columns.updatedAt')"
          min-width="200"
        >
          <template #default="{ row }">
            <div class="updated-at-cell">
              <ArtSvgIcon icon="ri:time-line" class="time-icon text-g-500" />
              <span>{{ formatDate(row.updated_at) }}</span>
            </div>
          </template>
        </ElTableColumn>
        <ElTableColumn
          :label="$t('configCenter.columns.operation')"
          width="260"
          fixed="right"
        >
          <template #default="{ row }">
            <CommonInlineRow gap="8px">
              <AuthorizationTooltipButton
                link
                type="primary"
                icon="ri:edit-line"
                :disabled="authorizationDisabled"
                :disabled-reason="authorizationDisabledReason"
                @click="handleEdit(row)"
              >
                {{ $t("configCenter.actions.edit") }}
              </AuthorizationTooltipButton>
              <ElButton link type="primary" @click="handleHistory(row)">
                <ArtSvgIcon icon="ri:history-line" class="mr-1" />
                {{ $t("configCenter.actions.history") }}
              </ElButton>
              <AuthorizationTooltipButton
                link
                type="danger"
                icon="ri:delete-bin-line"
                :disabled="authorizationDisabled"
                :disabled-reason="authorizationDisabledReason"
                @click="handleDelete(row)"
              >
                {{ $t("common.actions.delete") }}
              </AuthorizationTooltipButton>
            </CommonInlineRow>
          </template>
        </ElTableColumn>
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
import { ref, watch } from "vue";
import type { ListFilter } from "@/types/list-query";
import { configCenterCatalog } from "@/list-catalog/common-resources";
import { $t } from "@/locales";
import type { ConfigEntry } from "@/types/api/config-center";
import { deleteConfig } from "@/api/config-center";
import AuthorizationTooltipButton from "@/components/common/authorization-tooltip-button/index.vue";
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import ConfigEditDialog from "./modules/ConfigEditDialog.vue";
import HistoryDrawer from "./modules/HistoryDrawer.vue";
import ImportDialog from "./modules/ImportDialog.vue";
import ExportDialog from "./modules/ExportDialog.vue";
import NodeStatusCard from "./modules/NodeStatusCard.vue";
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
import type { DefaultRow } from "element-plus/es/components/table/src/table/defaults";
defineOptions({ name: "ConfigCenter" });
const {
  columns,
  loading,
  tableData,
  currentConfig,
  selectedRows,
  searchForm,
  pagination,
  existingServiceNames,
  formatDate,
  handleSelectionChange,
  clearSelection,
  handleDeleteCommand,
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
  selectedRows,
  searchForm,
  pagination,
  clearSelection,
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
      await refresh.refresh({
        resetSearch: true,
        resetPage: true,
        clearSelection: true,
      });
    }
    if (kind === "rollback") {
      await refresh.refresh({ clearSelection: true });
    }
    if (kind === "import") {
      await refresh.refresh({ clearSelection: true });
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
const handleTableSelectionChange = (rows: DefaultRow[]) => {
  handleSelectionChange(rows as ConfigEntry[]);
};
const handleDelete = async (row: ConfigEntry) => {
  await handleDeleteRow(
    row,
    (item) => deleteConfig(item.service_name),
    () => refresh.refresh({ clearSelection: true }),
  );
};
const handleDeleteAction = async (type: "all" | "selected") => {
  await handleDeleteCommand(
    type,
    tableData.value,
    (item) => deleteConfig(item.service_name),
    () => refresh.refresh({ clearSelection: true }),
  );
};
const syncTableScroll = () => {
  nextTick(() => {
    const tableEl = document.querySelector(".surface-panel-table");
    if (!tableEl) return;
    const bodyWrapper = tableEl.querySelector(".el-scrollbar__wrap");
    const headerWrapper = tableEl.querySelector(".el-table__header-wrapper");
    if (bodyWrapper && headerWrapper) {
      (bodyWrapper as HTMLElement).onscroll = () => {
        (headerWrapper as HTMLElement).scrollLeft = (
          bodyWrapper as HTMLElement
        ).scrollLeft;
      };
    }
  });
};
watch(
  tableData,
  () => {
    syncTableScroll();
  },
  { deep: true },
);
onMounted(() => {
  void loadLicenseStatus();
  refresh.initialize();
  syncTableScroll();
});
</script>

<style scoped lang="scss">
.art-table-card {
  flex: none;

  :deep(.el-card__body) {
    height: auto;
    overflow: visible;
  }

  :deep(.art-table) {
    height: auto !important;
    min-height: 0 !important;
  }
}

.tab-segmented {
  :deep(.el-segmented) {
    height: 40px;
    padding: 4px;
  }
}

.toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: flex-end;
  gap: 10px;
  padding: 12px 16px;
  background: var(--default-box-color);
  border: 1px solid var(--art-card-border);
  border-radius: calc(var(--custom-radius) + 2px);
}

.service-name-cell {
  display: flex;
  align-items: center;
  gap: 10px;

  .service-icon {
    font-size: 20px;
    flex-shrink: 0;
  }

  .service-info {
    display: flex;
    flex-direction: column;
    gap: 2px;

    .service-name {
      font-weight: 500;
      color: var(--art-gray-900);
      font-size: 14px;
    }
  }
}

.updated-by-cell,
.updated-at-cell {
  display: flex;
  align-items: center;
  gap: 6px;

  .user-icon,
  .time-icon {
    font-size: 16px;
  }

  span {
    font-size: 14px;
    color: var(--art-gray-700);
  }
}

@media (max-width: 768px) {
  .tab-segmented {
    width: 100%;
  }

  .toolbar {
    .toolbar-select {
      width: 100%;
      max-width: none;
    }

    .toolbar-divider {
      display: none;
    }
  }
}
</style>
