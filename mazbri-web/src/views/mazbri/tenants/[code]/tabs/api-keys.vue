<template>
  <TenantTabShell>
    <ResourceTableShell
      v-model:columns="apiKeyColumns"
      :title="$t('tenant.tabs.apiKeys')"
      :loading="loading"
      :data="apiKeys"
      :show-toolbar="false"
      stripe
      border
    >
      <template v-if="canWriteBusiness && apiKeyActions.create" #actions>
        <ElButton
          v-roles="ROLES_BIZ"
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
            v-if="apiKeyActions.rotate !== false"
            v-roles="ROLES_BIZ"
            link
            type="primary"
            :loading="rotatingPrefix === row.key_prefix"
            @click="handleRotate(row)"
          >
            {{ $t("tenant.actions.rotateApiKey") }}
          </ElButton>
          <ElButton
            v-roles="ROLES_BIZ"
            link
            type="primary"
            @click="handleRenew(row)"
          >
            {{ $t("common.actions.renew") }}
          </ElButton>
          <ElButton
            v-if="apiKeyActions.delete"
            v-roles="ROLES_BIZ"
            link
            type="danger"
            @click="handleRevoke(row)"
          >
            {{ $t("tenant.actions.revokeApiKey") }}
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
            {{ $t("common.units.days") }} ({{ $t("common.defaults.oneYear") }})
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
      :title="
        secretDialogMode === 'rotate'
          ? $t('tenant.apiKey.rotateTitle')
          : $t('tenant.apiKey.createTitle')
      "
      width="560px"
      destroy-on-close
    >
      <ElAlert
        type="warning"
        show-icon
        :closable="false"
        :title="$t('tenant.apiKey.secretWarning')"
      />
      <div class="mt-4 font-semibold">{{ $t("tenant.apiKey.secretLabel") }}</div>
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
        <ElButton @click="handleDownloadCreatedKey('txt')">
          {{ $t("tenant.apiKey.downloadTxt") }}
        </ElButton>
        <ElButton @click="handleDownloadCreatedKey('json')">
          {{ $t("tenant.apiKey.downloadJson") }}
        </ElButton>
        <ElButton @click="createdKeyVisible = false">{{
          $t("common.actions.close")
        }}</ElButton>
      </template>
    </CenteredDialog>
  </TenantTabShell>
</template>
<script setup lang="ts">
import { copyWithFeedback } from "@/utils/ui/copy-feedback";
import TenantTabShell from "@/components/common/tenant-tab-shell/index.vue";
import type { FormInstance, FormRules } from "element-plus";
import { ElMessage, ElMessageBox } from "element-plus";
import type {
  ApiKey,
  CreateApiKeyRequest,
  UpdateApiKeyRequest,
} from "@/types/api/api-key";
import {
  fetchTenantApiKeys,
  createApiKey,
  rotateApiKey,
  deleteApiKey,
} from "@/api/tenant";
import { updateApiKey as updateApiKeyApi } from "@/api/api-key";
import { $t } from "@/locales";
import { ROLES_BIZ } from "@/utils/ops/roles";
import { useUserStore } from "@/store/modules/user";
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
const userStore = useUserStore();
const canWriteBusiness = computed(() => userStore.canWriteBusiness);
const loading = ref(false);
const creating = ref(false);
const renewing = ref(false);
const rotatingPrefix = ref<string | null>(null);
const secretDialogMode = ref<"create" | "rotate">("create");
const createdKeyVal = ref("");
const createdKeyVisible = ref(false);
const apiKeys = ref<ApiKey[]>([]);
const apiKeyActions = ref<{
  create: boolean;
  rotate?: boolean;
  delete: boolean;
}>({
  create: false,
  rotate: false,
  delete: false,
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
    width: 190,
    fixed: "right",
    useSlot: true,
  },
]);
const formRef = ref<FormInstance>();
const renewFormRef = ref<FormInstance>();
const createForm = reactive<{ expires_days: number }>({
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
      scopes: ["api:read", "api:write"],
      expires_days: createForm.expires_days,
    };
    const res = await createApiKey(props.tenantCode, data);
    ElMessage.success($t("tenant.messages.apiKeyCreated"));
    createVisible.value = false;
    secretDialogMode.value = "create";
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
const handleRotate = async (row: ApiKey) => {
  try {
    await ElMessageBox.confirm(
      $t("tenant.messages.rotateApiKeyConfirm"),
      $t("common.tips"),
      {
        confirmButtonText: $t("common.confirm"),
        cancelButtonText: $t("common.cancel"),
        type: "warning",
      },
    );
  } catch {
    return;
  }
  rotatingPrefix.value = row.key_prefix;
  try {
    const res = await rotateApiKey(props.tenantCode, row.key_prefix);
    ElMessage.success($t("tenant.messages.apiKeyRotated"));
    secretDialogMode.value = "rotate";
    createdKeyVal.value = res.api_key;
    createdKeyVisible.value = true;
    await loadData();
  } catch (error) {
    logger.error("Failed to rotate API key:", error);
  } finally {
    rotatingPrefix.value = null;
  }
};

const handleRevoke = async (row: ApiKey) => {
  try {
    await ElMessageBox.confirm(
      $t("tenant.messages.revokeApiKeyConfirm"),
      $t("common.tips"),
      {
        confirmButtonText: $t("common.confirm"),
        cancelButtonText: $t("common.cancel"),
        type: "warning",
      },
    );
    await deleteApiKey(props.tenantCode, row.key_prefix);
    ElMessage.success($t("tenant.messages.apiKeyRevoked"));
    loadData();
  } catch (error) {
    if (error !== "cancel") {
      logger.error("Failed to revoke API key:", error);
    }
  }
};
const copyKey = async (text: string) => {
  if (!text) {
    ElMessage.error($t("tenant.messages.apiKeyMissing"));
    return;
  }
  await copyWithFeedback(text);
};
const handleCopyCreatedKey = () => {
  copyKey(createdKeyVal.value);
};
const handleDownloadCreatedKey = (format: "txt" | "json") => {
  try {
    if (!createdKeyVal.value) {
      ElMessage.error($t("tenant.messages.apiKeyMissing"));
      return;
    }
    const isJson = format === "json";
    const content = isJson
      ? `${JSON.stringify(
          {
            tenant_code: props.tenantCode,
            api_key: createdKeyVal.value,
            note: $t("tenant.apiKey.downloadNote"),
          },
          null,
          2,
        )}\n`
      : `Tenant Code: ${props.tenantCode}\nAPI Key: ${createdKeyVal.value}\n\n${$t("tenant.apiKey.downloadNote")}\n`;
    const mime = isJson
      ? "application/json;charset=utf-8"
      : "text/plain;charset=utf-8";
    const blob = new Blob([content], { type: mime });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `api_key_${props.tenantCode}.${format}`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
    ElMessage.success($t("tenant.messages.apiKeyDownloaded"));
  } catch (error) {
    logger.error("Failed to download API key:", error);
    ElMessage.error($t("tenant.messages.apiKeyDownloadFailed"));
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
  border-radius: var(--art-form-control-radius-sm);
}
</style>
