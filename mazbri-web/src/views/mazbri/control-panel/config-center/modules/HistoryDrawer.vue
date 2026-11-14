<template>
  <AdaptiveDrawer
    v-model="visible"
    direction="rtl"
    size="760px"
    destroy-on-close
    modal-class="history-modal"
    drawer-class="history-drawer"
    @close="emit('update:modelValue', false)"
  >
    <template #header>
      <CommonDrawerHeader
        :title="$t('configCenter.historyDrawer.title')"
        :subtitle="props.config?.service_name || ''"
      />
    </template>

    <div v-if="config" v-loading="loading" class="history-drawer__shell">
      <template v-if="!loading && historyData.length === 0">
        <ElAlert
          :title="$t('configCenter.historyDrawer.initialVersionNotice')"
          type="info"
          :closable="false"
          show-icon
          class="history-initial-alert"
        />

        <div class="active-preview-section">
          <div class="active-preview-section__title">
            {{ $t("configCenter.historyDrawer.activeConfigTitle") }}
          </div>
          <TomlEditor :content="activeContent" readonly height="440px" />
        </div>
      </template>

      <template v-else-if="historyData.length > 0">
        <div class="version-list">
          <ArtTable
            :loading="loading"
            :data="historyData"
            :columns="historyColumns"
            :pagination="artPagination"
            :max-height="240"
            empty-height="auto"
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

        <div v-if="selectedVersion" class="diff-section">
          <div class="diff-toolbar">
            <div class="diff-toolbar__left">
              <span class="diff-toolbar__label">
                {{ $t("configCenter.historyDrawer.compare.title") }}
              </span>
              <ElSelect v-model="compareTarget" class="diff-toolbar__select">
                <ElOption
                  :label="$t('configCenter.historyDrawer.compare.currentActive')"
                  value="current"
                />
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
            </div>

            <AuthorizationTooltipButton
              type="primary"
              plain
              :disabled="authorizationDisabled"
              :disabled-reason="authorizationDisabledReason"
              @click="handleRollback(selectedVersion)"
            >
              {{ $t("configCenter.historyDrawer.actions.rollbackToSelected") }}
            </AuthorizationTooltipButton>
          </div>

          <DiffViewer
            :old-content="selectedVersion.content"
            :new-content="comparedContent"
            :old-label="
              $t('configCenter.diffViewer.version', {
                version: selectedVersion.version_number,
              })
            "
            :new-label="comparedLabel"
          />
        </div>
      </template>
    </div>
  </AdaptiveDrawer>
</template>

<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage, ElMessageBox } from "element-plus";
import { $t } from "@/locales";
import AdaptiveDrawer from "@/components/common/AdaptiveDrawer.vue";
import CommonDrawerHeader from "@/components/common/drawer-header/index.vue";
import type { ConfigHistory, ConfigSummary } from "@/types/api/config-center";
import {
  fetchConfig,
  fetchConfigHistory,
  rollbackConfig,
} from "@/api/config-center";
import DiffViewer from "./DiffViewer.vue";
import TomlEditor from "./TomlEditor.vue";
import { getLicenseDisabledReason } from "@/composables/useLicenseGate";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-dialog-service";
import { logger } from "@/utils/sys";
import AuthorizationTooltipButton from "@/components/common/authorization-tooltip-button/index.vue";
import type { ColumnOption, PaginationConfig } from "@/types/component";
import { formatDateTime as formatDate } from "@/utils/format";

const props = defineProps<{
  modelValue: boolean;
  config: ConfigSummary | null;
  authorizationDisabled?: boolean;
  authorizationDisabledReason?: string;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: boolean];
  rollback: [];
}>();

const { locale } = useI18n();

const visible = computed({
  get: () => props.modelValue,
  set: (value: boolean) => emit("update:modelValue", value),
});

const authorizationDisabled = computed(
  () => props.authorizationDisabled ?? false,
);
const authorizationDisabledReason = computed(
  () => props.authorizationDisabledReason ?? getLicenseDisabledReason(),
);

const loading = ref(false);
const activeContent = ref("");
const historyData = ref<ConfigHistory[]>([]);
const selectedVersion = ref<ConfigHistory | null>(null);
const compareTarget = ref<"current" | number>("current");
let loadSeq = 0;

const pagination = reactive({
  page: 1,
  page_size: 10,
  total: 0,
});

const historyColumns = computed<ColumnOption<ConfigHistory>[]>(() => {
  void locale.value;
  return [
    {
      prop: "version_number",
      label: $t("configCenter.historyDrawer.columns.version"),
      width: 90,
    },
    {
      prop: "updated_by",
      label: $t("configCenter.historyDrawer.columns.updatedBy"),
      minWidth: 120,
    },
    {
      prop: "created_at",
      label: $t("configCenter.historyDrawer.columns.createdAt"),
      minWidth: 160,
      useSlot: true,
    },
    {
      prop: "operations",
      label: $t("common.fields.operations"),
      width: 100,
      useSlot: true,
    },
  ];
});

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

const comparedContent = computed(() => {
  if (compareTarget.value === "current") {
    return activeContent.value;
  }
  const found = historyData.value.find(
    (h) => h.version_number === compareTarget.value,
  );
  return found ? found.content : activeContent.value;
});

const comparedLabel = computed(() => {
  if (compareTarget.value === "current") {
    return $t("configCenter.diffViewer.currentActive");
  }
  return $t("configCenter.diffViewer.version", {
    version: compareTarget.value,
  });
});

const loadInitialData = async () => {
  const config = props.config;
  if (!config) return;
  const seq = ++loadSeq;
  loading.value = true;
  try {
    const [detail, historyRes] = await Promise.all([
      fetchConfig(config.service_name),
      fetchConfigHistory({
        service_name: config.service_name,
        page: pagination.page,
        page_size: pagination.page_size,
      }),
    ]);
    if (seq !== loadSeq || !visible.value) {
      return;
    }
    activeContent.value = detail.content;
    historyData.value = historyRes.history;
    pagination.total = historyRes.total;
    selectedVersion.value = historyRes.history[0] ?? null;
    compareTarget.value = "current";
  } catch (error: unknown) {
    if (seq !== loadSeq) {
      return;
    }
    if (!isMfaStepUpCancelled(error)) {
      logger.error("Failed to load config history:", error);
    }
    visible.value = false;
  } finally {
    if (seq === loadSeq) {
      loading.value = false;
    }
  }
};

const loadHistoryPage = async () => {
  const config = props.config;
  if (!config) return;
  const seq = ++loadSeq;
  loading.value = true;
  try {
    const historyRes = await fetchConfigHistory({
      service_name: config.service_name,
      page: pagination.page,
      page_size: pagination.page_size,
    });
    if (seq !== loadSeq || !visible.value) {
      return;
    }
    historyData.value = historyRes.history;
    pagination.total = historyRes.total;
    selectedVersion.value = historyRes.history[0] ?? null;
    compareTarget.value = "current";
  } catch (error: unknown) {
    if (seq !== loadSeq) {
      return;
    }
    if (!isMfaStepUpCancelled(error)) {
      logger.error("Failed to load config history page:", error);
    }
  } finally {
    if (seq === loadSeq) {
      loading.value = false;
    }
  }
};

const handlePageSizeChange = (size: number) => {
  pagination.page_size = size;
  pagination.page = 1;
  void loadHistoryPage();
};

const handleCurrentPageChange = (page: number) => {
  pagination.page = page;
  void loadHistoryPage();
};

const handleSelectVersion = (row: ConfigHistory | null) => {
  if (!row) {
    return;
  }
  selectedVersion.value = row;
  if (compareTarget.value === row.version_number) {
    compareTarget.value = "current";
  }
};

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
  } catch (error: unknown) {
    if (error !== "cancel" && !isMfaStepUpCancelled(error)) {
      logger.error("Failed to rollback:", error);
    }
  }
};

watch(
  () => [props.modelValue, props.config] as const,
  ([isOpen, targetConfig]) => {
    if (!isOpen || !targetConfig) {
      loadSeq++;
      loading.value = false;
      return;
    }
    pagination.page = 1;
    activeContent.value = "";
    historyData.value = [];
    selectedVersion.value = null;
    compareTarget.value = "current";
    void loadInitialData();
  },
  { immediate: true },
);
</script>

<style scoped lang="scss">
.history-drawer {
  :deep(.el-drawer__body) {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    padding-top: 8px;
  }
}

.history-drawer__shell {
  height: 100%;
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-height: 0;
  overflow-y: auto;
}

.history-initial-alert {
  flex-shrink: 0;
}

.active-preview-section {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-height: 0;
}

.active-preview-section__title {
  font-size: 13px;
  font-weight: 600;
  color: var(--el-text-color-primary);
}

.version-list {
  flex-shrink: 0;
  min-height: 0;
}

.diff-section {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-height: 300px;
}

.diff-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  flex-shrink: 0;
}

.diff-toolbar__left {
  display: flex;
  align-items: center;
  gap: 8px;
}

.diff-toolbar__label {
  font-size: 13px;
  font-weight: 500;
  color: var(--el-text-color-regular);
  white-space: nowrap;
}

.diff-toolbar__select {
  width: 180px;
}
</style>
