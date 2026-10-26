<template>
  <div class="whitelist-tab">
    <ResourceTableShell
      v-model:columns="whitelistColumns"
      :loading="loading"
      :data="entries"
      :show-toolbar="false"
      stripe
      border
      @selection-change="handleSelectionChange"
    >
      <template #headerLeft>
        <div class="surface-card-left">
          <span>{{ $t("tenant.whitelist.title") }}</span>
          <ElTag type="info" size="small" class="ml-2">
            {{ entries.length }} {{ $t("tenant.whitelist.ruleSuffix") }}
          </ElTag>
        </div>
      </template>
      <template #actions>
        <TableExportDropdown
          :items="exportItems"
          :label="$t('tenant.whitelist.export')"
          @command="(type) => handleExport(type as 'all' | 'selected')"
        />
        <ElButton v-roles="['R_ADMIN', 'R_FINANCE']" @click="handleImport">
          <template #icon><ArtSvgIcon icon="ri:upload-line" /></template>
          {{ $t("tenant.whitelist.import") }}
        </ElButton>
        <ElButton
          v-roles="['R_ADMIN', 'R_FINANCE']"
          type="primary"
          @click="handleAdd"
        >
          <template #icon><ArtSvgIcon icon="ri:add-line" /></template>
          {{ $t("tenant.whitelist.add") }}
        </ElButton>
        <TableDeleteDropdown
          v-roles="['R_ADMIN', 'R_FINANCE']"
          :total-count="entries.length"
          :selected-count="selectedRows.length"
          @command="(type) => handleDeleteCommand(type, entries)"
        />
      </template>

      <template #empty>
        <ElEmpty
          v-if="!loading"
          :description="$t('tenant.whitelist.emptyTitle')"
        >
          <template #description>
            <CommonInlineHint block size="sm">{{
              $t("tenant.whitelist.emptyHint1")
            }}</CommonInlineHint>
            <CommonInlineHint block>{{
              $t("tenant.whitelist.emptyHint2")
            }}</CommonInlineHint>
          </template>
        </ElEmpty>
      </template>

      <template #ip_address="{ row }">
        <code class="ip-code">{{ formatIpWithCidr(row) }}</code>
      </template>
      <template #cidr="{ row }">
        <ElTag :type="getCidrTagType(row.cidr)" size="small">
          {{ getCidrLabel(row.cidr) }}
        </ElTag>
      </template>
      <template #description="{ row }">
        {{ row.description || "-" }}
      </template>
      <template #is_active="{ row }">
        <StatusIndicator :type="row.is_active ? 'success' : 'info'">
          {{
            row.is_active
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
          <ElButton
            v-roles="['R_ADMIN', 'R_FINANCE']"
            link
            type="danger"
            @click="handleDelete(row)"
          >
            {{ $t("common.actions.delete") }}
          </ElButton>
        </TableActionRow>
      </template>
    </ResourceTableShell>

    <div v-if="selectedRows.length > 0" class="selection-footer">
      <ElTag type="info">{{
        $t("tenant.whitelist.selectedCount", { count: selectedRows.length })
      }}</ElTag>
      <ElButton
        v-roles="['R_ADMIN', 'R_FINANCE']"
        type="danger"
        link
        class="ml-3"
        @click="handleDeleteCommand('selected', entries)"
      >
        {{ $t("tenant.whitelist.batchDelete") }}
      </ElButton>
    </div>

    <EditDialog
      v-model="addVisible"
      :title="$t('tenant.whitelist.addDialog.title')"
      width="550px"
      :confirm-text="$t('tenant.whitelist.addDialog.confirm')"
      :saving="adding"
      dialog-class="whitelist-dialog"
      @confirm="handleSubmitAdd"
    >
      <ElForm
        ref="formRef"
        :model="addForm"
        :rules="addRules"
        label-width="100px"
        class="add-form"
      >
        <ElFormItem
          :label="$t('tenant.whitelist.addDialog.ipAddresses')"
          prop="ip_addresses"
        >
          <ElInput
            v-model="addForm.ip_addresses"
            type="textarea"
            :rows="4"
            :placeholder="
              $t('tenant.whitelist.addDialog.ipAddressesPlaceholder')
            "
            class="ip-input"
          />
          <CommonInlineHint block class="mt-1.5">
            {{ $t("tenant.whitelist.addDialog.ipAddressesHint") }}
          </CommonInlineHint>
        </ElFormItem>
        <ElFormItem
          :label="$t('tenant.whitelist.addDialog.description')"
          prop="description"
        >
          <ElInput
            v-model="addForm.description"
            :placeholder="
              $t('tenant.whitelist.addDialog.descriptionPlaceholder')
            "
            maxlength="100"
            show-word-limit
          />
        </ElFormItem>
      </ElForm>
    </EditDialog>

    <ImportDialog
      v-model="importVisible"
      :title="$t('tenant.whitelist.importDialog.title')"
      :columns="whitelistImportColumns"
      :format-hint="whitelistImportFormatHint"
      unique-key="key"
      :existing-data="whitelistImportExistingData"
      :validator="validateWhitelistImportRow"
      :transformer="transformWhitelistImportRow"
      :on-import="handleWhitelistImport"
      :show-success-message="false"
      :show-error-message="false"
      @success="loadData"
    />
  </div>
</template>

<script setup lang="ts">
import type { FormInstance, FormRules } from "element-plus";
import { ElMessage, ElMessageBox } from "element-plus";
import { $t } from "@/locales";
import type { WhitelistEntry, AddToWhitelistRequest } from "@/types/api/tenant";
import {
  fetchTenantWhitelist,
  addToWhitelist,
  removeFromWhitelist,
} from "@/api/tenant";
import EditDialog from "@/components/common/EditDialog.vue";
import ImportDialog from "@/components/common/ImportDialog.vue";
import CommonInlineHint from "@/components/common/inline-hint/index.vue";
import ResourceTableShell from "@/components/common/resource-table-shell/index.vue";
import StatusIndicator from "@/components/common/StatusIndicator.vue";
import TableDeleteDropdown from "@/components/table/TableDeleteDropdown.vue";
import TableExportDropdown from "@/components/table/TableExportDropdown.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { useTableSelection } from "@/composables/useTableSelection";
import { logger } from "@/utils/sys";
import { formatDateTime as formatDate } from "@/utils/format";
import { exportResourceCsv } from "@/composables/useResourceCsvImportExport";
import type { ColumnConfig } from "@/composables/useResourceCsvImportExport";
import type { ColumnOption } from "@/types/component";
defineOptions({ name: "WhitelistTab" });
type WhitelistExportRow = Record<string, unknown> & {
  ip_address: string;
  cidr: number;
  description: string;
};
const props = defineProps<{
  tenantCode: string;
}>();
const loading = ref(false);
const adding = ref(false);
const entries = ref<WhitelistEntry[]>([]);
const addVisible = ref(false);
const importVisible = ref(false);
const whitelistImportColumns: ColumnConfig[] = [
  {
    key: "ip_address",
    label: $t("tenant.whitelist.columns.ipAddress"),
    required: true,
  },
  {
    key: "cidr",
    label: $t("tenant.whitelist.columns.cidr"),
    required: true,
    type: "number",
  },
  {
    key: "description",
    label: $t("tenant.whitelist.columns.description"),
    required: false,
  },
];
const whitelistImportFormatHint = computed(() =>
  [
    $t("tenant.whitelist.importDialog.formatHint"),
    $t("tenant.whitelist.importDialog.formatExample"),
  ].join("\n"),
);
const whitelistImportExistingData = computed(() =>
  entries.value.map((entry) => {
    const ip = entry.ip_address.split("/")[0];
    return {
      key: `${ip}/${entry.cidr}`,
    };
  }),
);
const whitelistColumns = ref<ColumnOption<WhitelistEntry>[]>([
  { type: "selection", width: 55 },
  {
    prop: "ip_address",
    label: $t("tenant.whitelist.columns.ipAddress"),
    minWidth: 120,
    useSlot: true,
  },
  {
    prop: "cidr",
    label: $t("tenant.whitelist.columns.type"),
    width: 120,
    useSlot: true,
  },
  {
    prop: "description",
    label: $t("tenant.whitelist.columns.description"),
    minWidth: 100,
    useSlot: true,
  },
  {
    prop: "is_active",
    label: $t("tenant.whitelist.columns.status"),
    width: 80,
    useSlot: true,
  },
  {
    prop: "created_at",
    label: $t("common.fields.createdAt"),
    width: 160,
    useSlot: true,
  },
  {
    prop: "operations",
    label: $t("common.fields.operations"),
    width: 100,
    fixed: "right",
    useSlot: true,
  },
]);
const { selectedRows, handleSelectionChange, handleDeleteCommand } =
  useTableSelection<WhitelistEntry>({
    deleteApi: async (row) => {
      const ipIdentifier = `${row.ip_address.split("/")[0]}/${row.cidr}`;
      await removeFromWhitelist(props.tenantCode, ipIdentifier);
    },
    getItemName: (row) => `${row.ip_address}/${row.cidr}`,
    onDeleteSuccess: () => loadData(),
    allDeleteConfirmMsg: (count) =>
      $t("tenant.whitelist.deleteConfirmAll", { count }),
    selectedDeleteConfirmMsg: (count) =>
      $t("tenant.whitelist.deleteConfirmSelected", { count }),
  });
const exportItems = computed(() => [
  { command: "all", label: $t("tenant.whitelist.exportAll") },
  {
    command: "selected",
    label: $t("tenant.whitelist.exportSelected", {
      count: selectedRows.value.length,
    }),
    disabled: selectedRows.value.length === 0,
  },
]);
const formRef = ref<FormInstance>();
const addForm = reactive({
  ip_addresses: "",
  description: "",
});
const validateIpAddresses = (
  _rule: unknown,
  value: string,
  callback: (error?: Error) => void,
) => {
  if (!value || !value.trim()) {
    callback(new Error($t("tenant.whitelist.validation.ipRequired")));
    return;
  }
  const lines = value
    .trim()
    .split("\n")
    .filter((line) => line.trim());
  const ipv4WithCidrRegex = /^(\d{1,3}\.){3}\d{1,3}(\/\d{1,2})?$/;
  for (const line of lines) {
    const entry = line.trim();
    if (!ipv4WithCidrRegex.test(entry)) {
      callback(
        new Error($t("tenant.whitelist.validation.invalidFormat", { entry })),
      );
      return;
    }
    const [ipPart, cidrPart] = entry.split("/");
    const parts = ipPart.split(".");
    for (const part of parts) {
      const num = parseInt(part, 10);
      if (num < 0 || num > 255) {
        callback(
          new Error($t("tenant.whitelist.validation.ipRange", { entry })),
        );
        return;
      }
    }
    if (cidrPart) {
      const cidr = parseInt(cidrPart, 10);
      if (cidr < 0 || cidr > 32) {
        callback(
          new Error($t("tenant.whitelist.validation.cidrRange", { entry })),
        );
        return;
      }
    }
  }
  callback();
};
const addRules: FormRules = {
  ip_addresses: [
    { required: true, validator: validateIpAddresses, trigger: "blur" },
  ],
  description: [
    {
      required: true,
      message: $t("tenant.whitelist.validation.descriptionRequired"),
      trigger: "blur",
    },
  ],
};
const formatIpWithCidr = (row: WhitelistEntry) => {
  const ip = row.ip_address.split("/")[0];
  if (row.cidr === 32) {
    return ip;
  }
  return `${ip}/${row.cidr}`;
};
const getCidrLabel = (cidr: number) => {
  if (cidr === 32) return $t("tenant.whitelist.cidrLabels.singleIp");
  if (cidr >= 24) return $t("tenant.whitelist.cidrLabels.cSegment", { cidr });
  if (cidr >= 16) return $t("tenant.whitelist.cidrLabels.bSegment", { cidr });
  return $t("tenant.whitelist.cidrLabels.aSegment", { cidr });
};
const getCidrTagType = (cidr: number): "success" | "warning" | "info" => {
  if (cidr === 32) return "success";
  if (cidr >= 24) return "warning";
  return "info";
};
const loadData = async () => {
  try {
    await runWithLoading(loading, async () => {
      const res = await fetchTenantWhitelist(props.tenantCode);
      entries.value = res.entries;
    });
  } catch (error) {
    logger.error("Failed to load whitelist:", error);
  }
};
const handleAdd = () => {
  addForm.ip_addresses = "";
  addForm.description = "";
  addVisible.value = true;
};
const handleSubmitAdd = async () => {
  const valid = await formRef.value?.validate().catch(() => false);
  if (!valid) return;
  const ipList = addForm.ip_addresses
    .trim()
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line);
  if (ipList.length === 0) {
    ElMessage.warning($t("tenant.messages.ipRequired"));
    return;
  }
  try {
    await runWithLoading(adding, async () => {
      const data: AddToWhitelistRequest = {
        ip_addresses: ipList,
        description: addForm.description || undefined,
      };
      const res = await addToWhitelist(props.tenantCode, data);
      if (res.total_added > 0) {
        ElMessage.success(
          $t("tenant.messages.whitelistAddedCount", { count: res.total_added }),
        );
      }
      if (res.total_errors > 0) {
        const errorDetails = res.errors
          .map((e) => `${e.ip_address}: ${e.error}`)
          .join("\n");
        ElMessage.warning({
          message: $t("tenant.messages.whitelistPartialFailure", {
            count: res.total_errors,
            details: errorDetails,
          }),
          duration: 5000,
        });
      }
      addVisible.value = false;
      loadData();
    });
  } catch (error) {
    logger.error("Failed to add whitelist:", error);
  }
};
const handleImport = () => {
  importVisible.value = true;
};
const isValidWhitelistIp = (ip: string): boolean => {
  const ipv4Regex = /^(\d{1,3}\.){3}\d{1,3}$/;
  if (!ipv4Regex.test(ip)) return false;
  return ip.split(".").every((part) => {
    const num = parseInt(part, 10);
    return num >= 0 && num <= 255;
  });
};
const isValidWhitelistCidr = (cidr: number): boolean => cidr >= 0 && cidr <= 32;
const validateWhitelistImportRow = (row: Record<string, string>) => {
  const ip = (row.ip_address || "").trim();
  const cidr = parseInt((row.cidr || "").trim() || "32", 10);
  const normalizedIp = ip.split("/")[0];
  if (!isValidWhitelistIp(normalizedIp)) {
    return {
      valid: false,
      reason: $t("tenant.whitelist.validation.invalidFormat", {
        entry: ip || normalizedIp,
      }),
    };
  }
  if (!isValidWhitelistCidr(cidr)) {
    return {
      valid: false,
      reason: $t("tenant.whitelist.validation.cidrRange", {
        entry: ip || normalizedIp,
      }),
    };
  }
  return { valid: true };
};
const transformWhitelistImportRow = (row: Record<string, string>) => {
  const ip = (row.ip_address || "").trim();
  const cidr = parseInt((row.cidr || "").trim() || "32", 10);
  const normalizedIp = ip.split("/")[0];
  const description = (row.description || "").trim();
  return {
    key: `${normalizedIp}/${isNaN(cidr) ? 32 : cidr}`,
    ip_address: normalizedIp,
    cidr: isNaN(cidr) ? 32 : cidr,
    description,
  };
};
const handleWhitelistImport = async (items: Record<string, unknown>[]) => {
  const typedItems = items as Array<{
    ip_address: string;
    cidr: number;
    description?: string;
  }>;
  const ipList = typedItems.map(
    (item) => `${item.ip_address.split("/")[0]}/${item.cidr}`,
  );
  const commonDescription =
    typedItems.find((item) => item.description)?.description ||
    $t("tenant.whitelist.importDialog.defaultDescription");
  try {
    const res = await addToWhitelist(props.tenantCode, {
      ip_addresses: ipList,
      description: commonDescription,
    });
    if (res.total_added > 0) {
      ElMessage.success(
        $t("tenant.whitelist.importDialog.importSuccess", {
          count: res.total_added,
        }),
      );
    }
    if (res.total_errors > 0) {
      ElMessage.warning(
        $t("tenant.whitelist.importDialog.importFailedCount", {
          count: res.total_errors,
        }),
      );
    }
  } catch (error) {
    logger.error("Failed to import whitelist:", error);
    throw error;
  }
};
const handleExport = (type: "all" | "selected") => {
  if (type === "all") {
    exportToCsv(entries.value);
  } else if (type === "selected") {
    exportToCsv(selectedRows.value);
  }
};
const exportToCsv = (data: WhitelistEntry[]) => {
  if (data.length === 0) {
    ElMessage.warning($t("feedback.import.noDataToExport"));
    return;
  }
  const rows: WhitelistExportRow[] = data.map((entry) => {
    const ip = entry.ip_address.split("/")[0];
    return {
      ip_address: entry.cidr === 32 ? ip : `${ip}/${entry.cidr}`,
      cidr: entry.cidr,
      description: entry.description || "",
    };
  });
  const date = new Date().toISOString().slice(0, 10);
  exportResourceCsv(
    rows,
    [
      { key: "ip_address", label: "ip_address" },
      { key: "cidr", label: "cidr" },
      { key: "description", label: "description" },
    ],
    `ip_whitelist_${props.tenantCode}_${date}.csv`,
  );
};
const handleDelete = async (row: WhitelistEntry) => {
  const ipIdentifier = `${row.ip_address.split("/")[0]}/${row.cidr}`;
  try {
    await ElMessageBox.confirm(
      $t("tenant.messages.deleteIpConfirm", { ip: ipIdentifier }),
      $t("feedback.batch.deleteConfirmTitle"),
      {
        confirmButtonText: $t("feedback.batch.confirmActionButton", {
          action: $t("common.actions.delete"),
        }),
        cancelButtonText: $t("common.cancel"),
        type: "warning",
      },
    );
    await removeFromWhitelist(props.tenantCode, ipIdentifier);
    ElMessage.success($t("common.feedback.deleteSuccess"));
    loadData();
  } catch (error) {
    if (error !== "cancel") {
      logger.error("Failed to delete whitelist:", error);
    }
  }
};
onMounted(() => {
  loadData();
});
</script>

<style scoped lang="scss">
.whitelist-tab {
  .ip-code {
    font-family: "SF Mono", "Monaco", "Inconsolata", monospace;
    background-color: var(--fill-color-light);
    padding: 2px 8px;
    border-radius: 4px;
    font-size: 13px;
  }

  .ml-2 {
    margin-left: 8px;
  }

  .ml-3 {
    margin-left: 12px;
  }

  .selection-footer {
    display: flex;
    align-items: center;
    padding: 8px 16px;
    background-color: var(--fill-color-light);
    border-top: 1px solid var(--border-color);
  }
}

:deep(.whitelist-dialog) {
  .add-form {
    .ip-input {
      :deep(.el-textarea__inner) {
        font-family: "SF Mono", "Monaco", "Inconsolata", monospace;
        font-size: 13px;
      }
    }
  }
}
</style>
