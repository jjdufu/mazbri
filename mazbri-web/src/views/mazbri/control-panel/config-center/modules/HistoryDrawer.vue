<template>
  <AdaptiveDrawer
    v-model="visible"
    :title="$t('configCenter.historyDrawer.title')"
    direction="rtl"
    size="min(61.33vw, 746px)"
    destroy-on-close
    modal-class="history-modal"
    drawer-class="history-drawer"
    @close="emit('update:modelValue', false)"
  >
    <div v-if="config" class="history-drawer__shell">
      <div class="version-list">
        <ArtTable
          :loading="loading"
          :data="historyData"
          :columns="historyColumns"
          :pagination="artPagination"
          stripe
          border
          highlight-current-row
          class="history-table"
          @current-change="handleSelectVersion"
          @pagination:current-change="handleCurrentPageChange"
          @pagination:size-change="handlePageSizeChange"
        >
          <template #created_at="{ row }">
            {{ formatDate(row.created_at) }}
          </template>
          <template #operations="{ row }">
            <TableActionRow>
              <AuthorizationTooltipButton
                link
                type="primary"
                :disabled="authorizationDisabled"
                :disabled-reason="authorizationDisabledReason"
                @click="handleRollback(row)"
              >
                {{ $t("configCenter.historyDrawer.actions.rollback") }}
              </AuthorizationTooltipButton>
            </TableActionRow>
          </template>
        </ArtTable>
      </div>

      <div v-if="selectedVersion && compareToVersion" class="diff-section mt-4">
        <ElDivider content-position="left">
          {{ $t("configCenter.historyDrawer.compare.title") }}
          <ElSelect
            v-model="compareToVersion"
            size="small"
            class="ml-2"
            style="width: 120px"
          >
            <ElOption
              v-for="h in comparableVersions"
              :key="h.version_number"
              :label="
                $t('configCenter.historyDrawer.compare.versionLabel', {
                  version: h.version_number,
                })
              "
              :value="h.version_number"
            />
          </ElSelect>
        </ElDivider>

        <DiffViewer
          v-if="oldContent && newContent"
          :old-content="oldContent"
          :new-content="newContent"
          :old-version="compareToVersion"
          :new-version="selectedVersion.version_number"
        />
      </div>

      <ElEmpty
        v-else-if="!selectedVersion"
        :description="$t('configCenter.historyDrawer.empty')"
      />
    </div>
  </AdaptiveDrawer>
</template>

<script setup lang="ts">
import { ElMessageBox, ElMessage } from "element-plus";
import { $t } from "@/locales";
import AdaptiveDrawer from "@/components/common/AdaptiveDrawer.vue";
import type { ConfigEntry, ConfigHistory } from "@/types/api/config-center";
import {
  fetchConfigHistory,
  fetchConfigHistoryVersion,
  rollbackConfig,
} from "@/api/config-center";
import DiffViewer from "./DiffViewer.vue";
import { getLicenseDisabledReason } from "@/composables/useLicenseGate";
import { logger } from "@/utils/sys";
import { runWithLoading } from "@/composables/useAsyncState";
import AuthorizationTooltipButton from "@/components/common/authorization-tooltip-button/index.vue";
import type { ColumnOption, PaginationConfig } from "@/types/component";
import { formatDateTime as formatDate } from "@/utils/format";
const props = defineProps<{
  modelValue: boolean;
  config: ConfigEntry | null;
  authorizationDisabled?: boolean;
  authorizationDisabledReason?: string;
}>();
const emit = defineEmits<{
  "update:modelValue": [value: boolean];
  rollback: [];
}>();
const visible = computed({
  get: () => props.modelValue,
  set: (value: boolean) => emit("update:modelValue", value),
});
const authorizationDisabled = computed(
  () => props.authorizationDisabled ?? false,
);
const authorizationDisabledReason = computed(
  () =>
    props.authorizationDisabledReason ?? getLicenseDisabledReason(),
);
const loading = ref(false);
const historyData = ref<ConfigHistory[]>([]);
const selectedVersion = ref<ConfigHistory | null>(null);
const compareToVersion = ref<number | null>(null);
const oldContent = ref("");
const newContent = ref("");
const pagination = reactive({
  page: 1,
  page_size: 10,
  total: 0,
});
const historyColumns = ref<ColumnOption<ConfigHistory>[]>([
  {
    prop: "version_number",
    label: $t("configCenter.historyDrawer.columns.version"),
    width: 80,
  },
  {
    prop: "updated_by",
    label: $t("configCenter.historyDrawer.columns.updatedBy"),
    width: 120,
  },
  {
    prop: "created_at",
    label: $t("configCenter.historyDrawer.columns.createdAt"),
    minWidth: 120,
    useSlot: true,
  },
  {
    prop: "operations",
    label: $t("common.fields.operations"),
    minWidth: 100,
    useSlot: true,
  },
]);
const artPagination = computed<PaginationConfig>(() => ({
  current: pagination.page,
  size: pagination.page_size,
  total: pagination.total,
}));
const comparableVersions = computed(() => {
  if (!selectedVersion.value) return [];
  return historyData.value.filter(
    (h) => h.version_number !== selectedVersion.value!.version_number,
  );
});
const loadHistory = async () => {
  const config = props.config;
  if (!config) return;
  await runWithLoading(loading, async () => {
    const res = await fetchConfigHistory({
      service_name: config.service_name,
      page: pagination.page,
      page_size: pagination.page_size,
    });
    historyData.value = res.history;
    pagination.total = res.total;
  }).catch((error) => {
    logger.error("Failed to load history:", error);
  });
};
const handlePageSizeChange = (size: number) => {
  pagination.page_size = size;
  pagination.page = 1;
  loadHistory();
};
const handleCurrentPageChange = (page: number) => {
  pagination.page = page;
  loadHistory();
};
const handleSelectVersion = async (row: ConfigHistory | null) => {
  selectedVersion.value = row;
  if (row) {
    newContent.value = row.content;
    const prevVersion = historyData.value.find(
      (h) => h.version_number === row.version_number - 1,
    );
    if (prevVersion) {
      compareToVersion.value = prevVersion.version_number;
      oldContent.value = prevVersion.content;
    } else {
      compareToVersion.value = null;
      oldContent.value = "";
    }
  }
};
watch(compareToVersion, async (newVal) => {
  if (newVal && props.config) {
    const found = historyData.value.find((h) => h.version_number === newVal);
    if (found) {
      oldContent.value = found.content;
      return;
    }
    try {
      const version = await fetchConfigHistoryVersion({
        service_name: props.config.service_name,
        version_number: newVal,
      });
      oldContent.value = version.content;
    } catch (error) {
      logger.error("Failed to load version:", error);
    }
  }
});
const handleRollback = async (row: ConfigHistory) => {
  if (authorizationDisabled.value) {
    ElMessage.warning(authorizationDisabledReason.value);
    return;
  }
  try {
    await ElMessageBox.confirm(
      $t("configCenter.historyDrawer.confirmMessage", {
        version: row.version_number,
      }),
      $t("configCenter.historyDrawer.confirmTitle"),
      {
        confirmButtonText: $t("common.confirm"),
        cancelButtonText: $t("common.cancel"),
        type: "warning",
      },
    );
    await rollbackConfig({
      service_name: props.config!.service_name,
      version: row.version_number,
    });
    ElMessage.success(
      $t("configCenter.historyDrawer.messages.rollbackSuccess"),
    );
    emit("rollback");
  } catch (error) {
    if (error !== "cancel") {
      logger.error("Failed to rollback:", error);
    }
  }
};
watch(
  () => props.modelValue,
  (val) => {
    if (val && props.config) {
      pagination.page = 1;
      selectedVersion.value = null;
      compareToVersion.value = null;
      oldContent.value = "";
      newContent.value = "";
      loadHistory();
    }
  },
);
</script>

<style scoped lang="scss">
.history-drawer {
  :deep(.el-drawer__body) {
    display: flex;
    flex-direction: column;
    height: 100%;
    max-height: calc(100vh - 32px);
    overflow: hidden;
    padding-top: 0;
  }
}

.history-drawer__shell {
  height: 100%;
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.version-list {
  min-height: 0;

  :deep(.history-table) {
    width: 100%;

    .el-table__body-wrapper {
      .el-scrollbar__wrap {
        &::before,
        &::after {
          display: none;
        }
      }
    }
  }
}

.diff-section {
  flex: 1;
  min-height: 300px;
}
</style>
