<template>
  <div
    class="drafts-page"
    style="display: flex; flex-direction: column; flex: 1; min-height: 0"
  >
    <ResourceTableShell
      v-model:columns="columns"
      :title="$t('upstreamConnector.draftsTitle')"
      :loading="loading"
      :data="tableData"
      :empty-text="emptyDescription"
      :pagination="artPagination"
      :fit="true"
      table-layout="auto"
      stripe
      border
      @refresh="loadData"
      @selection-change="handleSelectionChange"
      @pagination:size-change="handleSizeChange"
      @pagination:current-change="handleCurrentChange"
    >
      <template #actions>
        <ResourceTableActionBar
          v-model="searchParams.q"
          :show-import="true"
          :import-label="$t('common.actions.import')"
          :import-roles="['R_ADMIN', 'R_OPERATOR']"
          :show-export="true"
          :export-items="exportItems"
          :export-disabled="tableData.length === 0"
          :export-roles="['R_ADMIN', 'R_OPERATOR']"
          :show-delete="true"
          :delete-total-count="tableData.length"
          :delete-selected-count="selectedRows.length"
          :delete-roles="['R_ADMIN', 'R_OPERATOR']"
          @search="handleSearch"
          @import="openImport"
          @export="handleExportCommand"
          @delete="(type) => handleDeleteCommand(type, tableData)"
        />
      </template>
      <template #endpoint_count="{ row }">
        {{ Array.isArray(row.endpoints) ? row.endpoints.length : 0 }}
      </template>
      <template #status="{ row }">
        <StatusIndicator :type="row.status === 'published' ? 'success' : 'info'">
          {{ statusLabel(row.status) }}
        </StatusIndicator>
      </template>
      <template #updated_at="{ row }">
        {{ formatDateTime(row.updated_at) }}
      </template>
      <template #operations="{ row }">
        <TableActionRow>
          <ElButton link type="primary" @click="openDraft(row)">
            {{ $t("upstreamConnector.continueEdit") }}
          </ElButton>
          <ElButton
            v-roles="['R_ADMIN', 'R_OPERATOR']"
            link
            type="danger"
            @click="handleDelete(row)"
          >
            {{ $t("common.actions.delete") }}
          </ElButton>
        </TableActionRow>
      </template>
    </ResourceTableShell>

    <WorkbenchImportDialog
      v-model="importVisible"
      v-model:import-tab="importTab"
      v-model:import-curl-text="importCurlText"
      v-model:import-json-text="importJsonText"
      v-model:mode="importMode"
      :importing="importing"
      :show-mode="false"
      @confirm="doImport"
    />
  </div>
</template>

<script setup lang="ts">
import { ElMessage, ElMessageBox } from "element-plus";
import { computed, onMounted, reactive, ref } from "vue";
import { useRouter } from "vue-router";
import { saveAs } from "file-saver";
import {
  deleteDraft,
  exportPostman,
  importCurl,
  importOpenApi,
  importPostman,
  listDrafts,
  type ConnectorDraft,
} from "@/api/upstream-connectors";
import StatusIndicator from "@/components/common/StatusIndicator.vue";
import ResourceTableShell from "@/components/common/resource-table-shell/index.vue";
import ResourceTableActionBar from "@/components/common/resource-table-action-bar/index.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { useTableSelection } from "@/composables/useTableSelection";
import type { ColumnOption, PaginationConfig } from "@/types/component";
import { $t } from "@/locales";
import { formatDateTime } from "@/utils/format";
import WorkbenchImportDialog from "./WorkbenchImportDialog.vue";

defineOptions({ name: "ConnectorDraftList" });

const router = useRouter();
const loading = ref(false);
const tableData = ref<ConnectorDraft[]>([]);
const searchParams = reactive({ q: "" });
const pagination = reactive({
  current: 1,
  size: 20,
  total: 0,
});
const importVisible = ref(false);
const importing = ref(false);
const importTab = ref("curl");
const importMode = ref("replace");
const importCurlText = ref("");
const importJsonText = ref("");

const columns = ref<ColumnOption[]>([
  { type: "selection", width: 55 },
  { prop: "name", label: $t("common.fields.name"), minWidth: 160 },
  {
    prop: "upstream_url",
    label: $t("supplier.fields.accessAddress"),
    minWidth: 220,
  },
  {
    prop: "endpoint_count",
    label: $t("upstreamConnector.endpointCount"),
    minWidth: 88,
    useSlot: true,
  },
  {
    prop: "status",
    label: $t("common.fields.status"),
    minWidth: 100,
    useSlot: true,
  },
  {
    prop: "updated_at",
    label: $t("common.fields.updatedAt"),
    minWidth: 180,
    useSlot: true,
  },
  {
    prop: "operations",
    label: $t("common.fields.operations"),
    width: 180,
    fixed: "right",
    useSlot: true,
  },
]);

const artPagination = computed<PaginationConfig>(() => ({
  current: pagination.current,
  size: pagination.size,
  total: pagination.total,
}));

const hasActiveSearch = computed(() => Boolean(searchParams.q.trim()));
const emptyDescription = computed(() =>
  hasActiveSearch.value
    ? $t("upstreamConnector.draftsNoMatch")
    : $t("upstreamConnector.draftsEmpty"),
);

const {
  selectedRows,
  handleSelectionChange,
  handleDeleteCommand,
  clearSelection,
} = useTableSelection<ConnectorDraft>({
  deleteApi: async (row) => {
    await deleteDraft(row.id);
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

function statusLabel(status: string) {
  if (status === "published") return $t("upstreamConnector.statusPublished");
  if (status === "draft") return $t("upstreamConnector.statusDraft");
  return status;
}

async function loadData() {
  await runWithLoading(loading, async () => {
    const res = await listDrafts({
      q: searchParams.q.trim() || undefined,
      page: pagination.current,
      page_size: pagination.size,
    });
    tableData.value = res?.drafts || [];
    pagination.total = res?.pagination?.total || 0;
    clearSelection();
  });
}

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

function openDraft(row: ConnectorDraft) {
  void router.push({
    name: "UpstreamConnectorWorkbench",
    query: { session: row.id },
  });
}

async function handleDelete(row: ConnectorDraft) {
  await ElMessageBox.confirm(
    $t("upstreamConnector.deleteDraftConfirm", { name: row.name }),
    $t("common.tips"),
    { type: "warning" },
  );
  await deleteDraft(row.id);
  ElMessage.success($t("common.feedback.deleteSuccess"));
  await loadData();
}

function openImport() {
  importMode.value = "replace";
  importVisible.value = true;
}

async function doImport() {
  importing.value = true;
  try {
    let d: ConnectorDraft;
    if (importTab.value === "curl") {
      d = await importCurl({ curl: importCurlText.value });
    } else if (importTab.value === "postman") {
      d = await importPostman({ collection: JSON.parse(importJsonText.value) });
    } else {
      d = await importOpenApi({ document: JSON.parse(importJsonText.value) });
    }
    importVisible.value = false;
    ElMessage.success($t("upstreamConnector.imported"));
    void router.push({
      name: "UpstreamConnectorWorkbench",
      query: { session: d.id },
    });
  } catch (e) {
    ElMessage.error((e as Error)?.message || $t("common.feedback.unknownError"));
  } finally {
    importing.value = false;
  }
}

async function handleExportCommand(command: string) {
  const rows = command === "selected" ? selectedRows.value : tableData.value;
  if (!rows.length) {
    ElMessage.warning(
      command === "selected"
        ? $t("feedback.batch.selectDataForAction", {
            action: $t("common.actions.export"),
          })
        : $t("feedback.batch.noDataForAction", {
            action: $t("common.actions.export"),
          }),
    );
    return;
  }
  for (const row of rows) {
    const col = await exportPostman(row.id);
    const blob = new Blob([JSON.stringify(col, null, 2)], {
      type: "application/json",
    });
    saveAs(blob, `${row.name || "connector"}.postman.json`);
  }
}

onMounted(() => {
  void loadData();
});
</script>

<style scoped lang="scss">
.drafts-page {
  padding: 0;
}
</style>
