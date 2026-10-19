<template>
  <div class="api-keys-tab">
    <ResourceTableShell
      v-model:columns="apiKeyColumns"
      :title="$t('tenant.tabs.apiKeys')"
      :loading="loading"
      :data="apiKeys"
      :show-toolbar="false"
      stripe
      border
    >
      <template #actions>
        <ElButton
          v-if="apiKeyActions.create"
          v-roles="['R_ADMIN', 'R_OPERATOR']"
          type="primary"
          @click="handleCreate"
        >
          <template #icon><ArtSvgIcon icon="ri:add-line" /></template>
          {{ $t("tenant.actions.addApiKey") }}
        </ElButton>
      </template>

      <template #key_suffix="{ row }">
        <code class="api-key-code">****{{ row.key_suffix }}</code>
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
      <template #expires_at="{ row }">
        {{
          row.expires_at
            ? formatDate(row.expires_at)
            : $t("common.status.neverExpires")
        }}
      </template>
      <template #operations="{ row }">
        <TableActionRow>
          <ElButton
            v-roles="['R_ADMIN', 'R_OPERATOR']"
            link
            type="success"
            @click="handleRenew(row)"
          >
            {{ $t("common.actions.renew") }}
          </ElButton>
          <ElButton
            v-if="apiKeyActions.delete"
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

    <EditDialog
      v-model="createVisible"
      :title="$t('tenant.apiKey.createTitle')"
      width="500px"
      :confirm-text="$t('common.confirm')"
      :saving="creating"
      @confirm="handleSubmitCreate"
    >
      <ElForm
        ref="formRef"
        :model="createForm"
        :rules="createRules"
        label-width="100px"
      >
        <ElFormItem :label="$t('tenant.apiKey.expiryDays')">
          <ElInputNumber
            v-model="createForm.expires_days"
            :min="1"
            :max="3650"
            :step="30"
          />
          <CommonInlineHint class="ml-2" size="sm">
            {{ $t("common.units.days") }}（{{ $t("common.defaults.oneYear") }}）
          </CommonInlineHint>
        </ElFormItem>

      </ElForm>
    </EditDialog>

    <EditDialog
      v-model="renewVisible"
      :title="$t('tenant.actions.renewApiKey')"
      width="450px"
      :confirm-text="$t('common.confirm')"
      :saving="renewing"
      @confirm="handleSubmitRenew"
    >
      <ElForm
        ref="renewFormRef"
        :model="renewForm"
        :rules="renewRules"
        label-width="100px"
      >
        <ElFormItem :label="$t('tenant.apiKey.key')">
          <ElInput :model-value="renewForm.keyPrefix" disabled />
        </ElFormItem>
        <ElFormItem :label="$t('tenant.apiKey.currentExpiry')">
          <ElInput :model-value="renewForm.currentExpires" disabled />
        </ElFormItem>
        <ElFormItem :label="$t('tenant.apiKey.renewDays')" prop="expires_days">
          <ElInputNumber
            v-model="renewForm.expires_days"
            :min="1"
            :max="3650"
            :step="30"
          />
          <CommonInlineHint class="ml-2" size="sm">{{
            $t("common.units.days")
          }}</CommonInlineHint>
        </ElFormItem>
      </ElForm>
    </EditDialog>

    <CenteredDialog
      v-model="createdKeyVisible"
      :title="$t('tenant.apiKey.createTitle')"
      width="550px"
      destroy-on-close
    >
      <ElAlert
        type="warning"
        show-icon
        :closable="false"
        title="密钥仅展示一次，请立即保存。"
      />
      <div class="mt-4 font-semibold">API Key:</div>
      <ElInput
        class="mt-2"
        type="textarea"
        :rows="3"
        readonly
        :model-value="createdKeyVal"
      />
      <template #footer>
        <ElButton type="primary" @click="handleCopyCreatedKey">{{
          $t("common.actions.copy")
        }}</ElButton>
        <ElButton type="success" @click="handleDownloadCreatedKey">
          下载
        </ElButton>
        <ElButton @click="createdKeyVisible = false">{{
          $t("common.actions.close")
        }}</ElButton>
      </template>
    </CenteredDialog>
  </div>
</template>

<script setup lang="ts">
import type { FormInstance, FormRules } from "element-plus";
import { ElMessage, ElMessageBox } from "element-plus";
import type {
  ApiKey,
  CreateApiKeyRequest,
  CreateApiKeyResponse,
  UpdateApiKeyRequest,
} from "@/types/api/api-key";
import { fetchTenantApiKeys, createApiKey, deleteApiKey } from "@/api/tenant";
import { updateApiKey as updateApiKeyApi } from "@/api/api-key";
import { $t } from "@/locales";
import EditDialog from "@/components/common/EditDialog.vue";
import CenteredDialog from "@/components/common/CenteredDialog.vue";
import CommonInlineHint from "@/components/common/inline-hint/index.vue";
import ResourceTableShell from "@/components/common/resource-table-shell/index.vue";
import StatusIndicator from "@/components/common/StatusIndicator.vue";
import { logger } from "@/utils/sys";
import { formatDateTime as formatDate } from "@/utils/format";
import type { ColumnOption } from "@/types/component";
defineOptions({ name: "ApiKeysTab" });
const props = defineProps<{
  tenantCode: string;
}>();
const loading = ref(false);
const creating = ref(false);
const renewing = ref(false);
const createdKeyVal = ref("");
const createdKeyVisible = ref(false);
const apiKeys = ref<ApiKey[]>([]);
const apiKeyActions = ref<{
  create: boolean;
  delete: boolean;
}>({
  create: true,
  delete: true,
});
const createVisible = ref(false);
const renewVisible = ref(false);
const apiKeyColumns = ref<ColumnOption<ApiKey>[]>([
  {
    prop: "key_suffix",
    label: $t("tenant.apiKey.key"),
    minWidth: 120,
    useSlot: true,
  },

  {
    prop: "is_active",
    label: $t("common.fields.status"),
    width: 100,
    useSlot: true,
  },
  {
    prop: "expires_at",
    label: $t("tenant.apiKey.expiry"),
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
]);
const formRef = ref<FormInstance>();
const renewFormRef = ref<FormInstance>();
const createForm = reactive<CreateApiKeyRequest>({
  scopes: [],
  expires_days: 365,
});
const renewForm = reactive({
  keyPrefix: "",
  currentExpires: "",
  expires_days: 365,
});
const createRules: FormRules = {};
const renewRules: FormRules = {
  expires_days: [
    {
      required: true,
      message: $t("tenant.apiKey.validation.renewDaysRequired"),
      trigger: "blur",
    },
    {
      type: "number",
      min: 1,
      max: 3650,
      message: $t("tenant.apiKey.validation.renewDaysRange"),
      trigger: "blur",
    },
  ],
};
const loadData = async () => {
  loading.value = true;
  try {
    const res = await fetchTenantApiKeys(props.tenantCode);
    apiKeys.value = res.keys;
    if (res.actions) {
      apiKeyActions.value = res.actions;
    }
  } catch (error) {
    logger.error("Failed to load API keys:", error);
  } finally {
    loading.value = false;
  }
};
const handleCreate = () => {
  createForm.expires_days = 365;
  createVisible.value = true;
};
const handleSubmitCreate = async () => {
  const valid = await formRef.value?.validate().catch(() => false);
  if (!valid) return;
  creating.value = true;
  try {
    const data: CreateApiKeyRequest = {
      scopes: ["read", "write", "admin"],
      expires_days: createForm.expires_days,
    };
    const res = await createApiKey(props.tenantCode, data);
    ElMessage.success($t("tenant.messages.apiKeyCreated"));
    createVisible.value = false;
    createdKeyVal.value = res.api_key;
    createdKeyVisible.value = true;
    loadData();
  } catch (error: unknown) {
    const apiError = error as {
      response?: {
        data?: {
          error?: string;
        };
      };
      message?: string;
    };
    if (apiError?.response?.data?.error === "api_key_already_exists") {
      ElMessage.warning($t("tenant.messages.apiKeyExists"));
    } else {
      logger.error("Failed to create API key:", error);
    }
  } finally {
    creating.value = false;
  }
};
const handleRenew = (row: ApiKey) => {
  renewForm.keyPrefix = row.display_key;
  renewForm.currentExpires = row.expires_at
    ? formatDate(row.expires_at)
    : $t("common.status.neverExpires");
  renewForm.expires_days = 365;
  renewVisible.value = true;
};

const handleSubmitRenew = async () => {
  const valid = await renewFormRef.value?.validate().catch(() => false);
  if (!valid) return;
  const row = apiKeys.value.find((k) => k.display_key === renewForm.keyPrefix);
  if (!row) return;
  renewing.value = true;
  try {
    const data: UpdateApiKeyRequest = {
      expires_days: renewForm.expires_days,
    };
    await updateApiKeyApi(props.tenantCode, row.key_prefix, data);
    ElMessage.success($t("tenant.messages.apiKeyRenewed"));
    renewVisible.value = false;
    loadData();
  } catch (error) {
    logger.error("Failed to renew API key:", error);
  } finally {
    renewing.value = false;
  }
};
const handleDelete = async (row: ApiKey) => {
  try {
    await ElMessageBox.confirm(
      $t("tenant.messages.deleteApiKeyConfirm"),
      $t("common.tips"),
      {
        confirmButtonText: $t("common.confirm"),
        cancelButtonText: $t("common.cancel"),
        type: "warning",
      },
    );
    await deleteApiKey(props.tenantCode, row.key_prefix);
    ElMessage.success($t("common.feedback.deleteSuccess"));
    loadData();
  } catch (error) {
    if (error !== "cancel") {
      logger.error("Failed to delete API key:", error);
    }
  }
};
const copyKey = async (text: string) => {
  if (!text) {
    ElMessage.error($t("tenant.messages.apiKeyMissing"));
    return;
  }
  try {
    if (navigator.clipboard && navigator.clipboard.writeText) {
      await navigator.clipboard.writeText(text);
      ElMessage.success($t("common.feedback.copiedToClipboard"));
      return;
    }
    const textarea = document.createElement("textarea");
    textarea.value = text;
    textarea.style.position = "fixed";
    textarea.style.opacity = "0";
    document.body.appendChild(textarea);
    textarea.select();
    document.execCommand("copy");
    document.body.removeChild(textarea);
    ElMessage.success($t("common.feedback.copiedToClipboard"));
  } catch {
    ElMessage.error($t("common.feedback.copyFailed"));
  }
};
const handleCopyCreatedKey = () => {
  copyKey(createdKeyVal.value);
};
const handleDownloadCreatedKey = () => {
  try {
    const text = `Tenant Code: ${props.tenantCode}\nAPI Key: ${createdKeyVal.value}\n\n请妥善保管此密钥。`;
    const blob = new Blob([text], { type: "text/plain;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `api_key_${props.tenantCode}.txt`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
    ElMessage.success("密钥已下载");
  } catch (error) {
    logger.error("Failed to download API key:", error);
    ElMessage.error("下载失败");
  }
};
onMounted(() => {
  loadData();
});
</script>

<style scoped lang="scss">
.api-key-code {
  font-family: monospace;
  background-color: var(--fill-color-light);
  padding: 2px 6px;
  border-radius: 4px;
}
</style>
