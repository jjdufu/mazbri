<template>
  <div class="product-detail-page">
    <ElPageHeader @back="router.back()">
      <template #content>
        <span class="text-lg font-medium">{{
          product?.name || $t("product.detail.pageTitle")
        }}</span>
      </template>
      <template #extra>
        <ElTag :type="product?.enabled ? 'success' : 'info'">
          {{
            product?.enabled
              ? $t("common.status.enabled")
              : $t("common.status.disabled")
          }}
        </ElTag>
      </template>
    </ElPageHeader>

    <ElCard v-loading="loading" class="mt-5" shadow="never">
      <template #header>
        <SurfaceCardHeader
          :title="$t('product.detail.basicInfoTitle')"
          stack-on-mobile
        >
          <template #actions>
            <ElButton
              v-roles="['R_ADMIN', 'R_OPERATOR']"
              type="primary"
              @click="handleEdit"
              >{{ $t("common.actions.modify") }}</ElButton
            >
            <ElButton
              v-roles="['R_ADMIN', 'R_OPERATOR']"
              type="danger"
              @click="handleDelete"
            >
              <template #icon
                ><ArtSvgIcon icon="ri:delete-bin-line"
              /></template>
              {{ $t("common.actions.delete") }}
            </ElButton>
          </template>
        </SurfaceCardHeader>
      </template>

      <ElDescriptions class="product-descriptions" :column="2" border>
        <ElDescriptionsItem :label="$t('product.fields.internalId')">{{
          product?.id
        }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('product.fields.productId')">
          <div class="product-code-value">
            <span>{{ product?.product_id }}</span>
            <ElTag size="small" effect="plain" type="info">{{
              $t("product.detail.lockedHint")
            }}</ElTag>
          </div>
        </ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('product.fields.name')">{{
          product?.name
        }}</ElDescriptionsItem>

        <ElDescriptionsItem :label="$t('product.fields.enabled')">
          <ElSwitch
            :model-value="product?.enabled"
            :loading="statusLoading"
            :disabled="!userStore.canWriteBusiness"
            @change="handleStatusChange"
          />
        </ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('common.fields.createdAt')">{{
          formatDate(product?.created_at || "")
        }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('common.fields.updatedAt')">{{
          formatDate(product?.updated_at || "")
        }}</ElDescriptionsItem>
      </ElDescriptions>

      <ElCard class="mt-5" shadow="never">
        <template #header>
          <SurfaceCardHeader
            :title="$t('product.detail.advancedInfoTitle')"
            stack-on-mobile
          />
        </template>

        <ElDescriptions class="product-descriptions" :column="2" border>
          <ElDescriptionsItem :label="$t('product.fields.timeoutSecs')">
            {{ product?.timeout_secs }}s
          </ElDescriptionsItem>
          <ElDescriptionsItem :label="$t('product.fields.cbProEnabled')">
            <ElTag :type="product?.cb_pro_enabled ? 'warning' : 'info'">
              {{
                product?.cb_pro_enabled
                  ? $t("common.status.enabled")
                  : $t("common.status.disabled")
              }}
            </ElTag>
          </ElDescriptionsItem>
          <ElDescriptionsItem
            v-if="product?.cb_pro_enabled"
            :label="$t('product.fields.cbFailureThreshold') || '失败熔断阈值'"
          >
            {{ product?.cb_failure_threshold }}
          </ElDescriptionsItem>
          <ElDescriptionsItem
            v-if="product?.cb_pro_enabled"
            :label="$t('product.fields.cbSuccessThreshold') || '成功恢复阈值'"
          >
            {{ product?.cb_success_threshold }}
          </ElDescriptionsItem>
          <ElDescriptionsItem
            v-if="product?.cb_pro_enabled"
            :label="$t('product.fields.cbCooldownSecs') || '熔断冷却时间'"
          >
            {{ product?.cb_cooldown_secs }}s
          </ElDescriptionsItem>
        </ElDescriptions>
      </ElCard>

      <EditDialog
        v-model="editVisible"
        :title="$t('product.detail.editTitle')"
        width="560px"
        :saving="saving"
        @confirm="handleSave"
      >
        <ElAlert
          :title="$t('product.detail.lockedAlert')"
          type="info"
          :closable="false"
          class="mb-4"
        />
        <ElForm
          ref="formRef"
          :model="editForm"
          :rules="rules"
          label-width="120px"
        >
          <div class="dialog-section">
            <p class="dialog-section-title">
              {{ $t("product.detail.basicInfoTitle") }}
            </p>
            <ElFormItem :label="$t('product.fields.name')" prop="name">
              <ElInput v-model="editForm.name" />
            </ElFormItem>

            <ElFormItem :label="$t('product.fields.enabled')">
              <ElSwitch v-model="editForm.enabled" />
            </ElFormItem>
          </div>

          <div class="dialog-section mt-4">
            <p class="dialog-section-title">
              {{ $t("product.detail.advancedInfoTitle") }}
            </p>
            <ElFormItem
              :label="$t('product.fields.timeoutSecs')"
              prop="timeout_secs"
            >
              <ElInputNumber
                v-model="editForm.timeout_secs"
                :min="1"
                :max="300"
                controls-position="right"
                style="width: 100%"
              />
            </ElFormItem>
            <ElFormItem
              :label="$t('product.fields.cbProEnabled')"
              prop="cb_pro_enabled"
            >
              <ElSwitch v-model="editForm.cb_pro_enabled" />
            </ElFormItem>
            <ElFormItem
              v-if="editForm.cb_pro_enabled"
              :label="$t('product.fields.cbFailureThreshold') || '失败熔断阈值'"
              prop="cb_failure_threshold"
            >
              <ElInputNumber
                v-model="editForm.cb_failure_threshold"
                :min="1"
                controls-position="right"
                style="width: 100%"
              />
            </ElFormItem>
            <ElFormItem
              v-if="editForm.cb_pro_enabled"
              :label="$t('product.fields.cbSuccessThreshold') || '成功恢复阈值'"
              prop="cb_success_threshold"
            >
              <ElInputNumber
                v-model="editForm.cb_success_threshold"
                :min="1"
                controls-position="right"
                style="width: 100%"
              />
            </ElFormItem>
            <ElFormItem
              v-if="editForm.cb_pro_enabled"
              :label="$t('product.fields.cbCooldownSecs') || '熔断冷却时间'"
              prop="cb_cooldown_secs"
            >
              <ElInputNumber
                v-model="editForm.cb_cooldown_secs"
                :min="1"
                controls-position="right"
                style="width: 100%"
              />
            </ElFormItem>
          </div>
        </ElForm>
      </EditDialog>
    </ElCard>

    <div class="mt-5">
      <ResourceTableShell
        :title="$t('product.detail.authorizedTenantsTitle')"
        :loading="tenantAccessLoading"
        :data="authorizedTenants"
        :columns="authorizedTenantColumns"
        :empty-text="$t('product.detail.noAuthorizedTenants')"
        :show-toolbar="false"
        stripe
        border
      >
        <template #actions>
          <ElTag type="info"
            >{{ authorizedTenants.length }}
            {{ $t("product.detail.tenantCountSuffix") }}</ElTag
          >
        </template>

        <template #status="{ row }">
          <StatusIndicator :type="getStatusType(row.status)">
            {{ getStatusLabel(row.status) }}
          </StatusIndicator>
        </template>
        <template #granted_at="{ row }">
          {{ formatDate(row.granted_at) }}
        </template>
        <template #operations="{ row }">
          <TableActionRow>
            <ElButton
              link
              type="primary"
              @click="handleViewTenant(row.tenant_code)"
            >
              {{ $t("common.actions.details") }}
            </ElButton>
          </TableActionRow>
        </template>
      </ResourceTableShell>
    </div>

    <div class="mt-5">
      <UserRouteGovernance
        embedded
        :initial-product-id="product?.id || ''"
        :lock-product="true"
      />
    </div>

  </div>
</template>

<script setup lang="ts">
import type { FormInstance, FormRules } from "element-plus";
import { ElMessageBox, ElMessage } from "element-plus";
import UserRouteGovernance from "@/views/mazbri/operations/user-routes/index.vue";
import { $t } from "@/locales";
import type { Product, UpdateProductRequest } from "@/types/api/product";
import type { TenantStatus } from "@/types/api/tenant";
import {
  fetchProductById,
  fetchProductTenants,
  updateProduct,
  deleteProduct,
} from "@/api/product";
import { fetchTenantList } from "@/api/tenant";
import {
  buildAuthorizedTenantRows,
  type AuthorizedTenantRow,
} from "../authorized-tenants";
import EditDialog from "@/components/common/EditDialog.vue";
import StatusIndicator from "@/components/common/StatusIndicator.vue";
import ResourceTableShell from "@/components/common/resource-table-shell/index.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { useUserStore } from "@/store/modules/user";
import { logger } from "@/utils/sys";
import { getStatusType, getStatusLabel } from "@/utils/display-labels";
import { formatDateTime as formatDate } from "@/utils/format";
import type { ColumnOption } from "@/types/component";
defineOptions({ name: "ProductDetail" });
const router = useRouter();
const route = useRoute();
const userStore = useUserStore();
const loading = ref(false);
const saving = ref(false);
const statusLoading = ref(false);
const editVisible = ref(false);
const product = ref<Product | null>(null);
const tenantAccessLoading = ref(false);
const authorizedTenants = ref<AuthorizedTenantRow[]>([]);
const authorizedTenantColumns = ref<ColumnOption<AuthorizedTenantRow>[]>([
  {
    prop: "name",
    label: $t("product.detail.tenantColumns.name"),
    minWidth: 112,
  },
  {
    prop: "tenant_id",
    label: $t("product.detail.tenantColumns.tenantId"),
    minWidth: 100,
  },
  {
    prop: "tenant_code",
    label: $t("product.detail.tenantColumns.tenantCode"),
    minWidth: 100,
  },
  {
    prop: "status",
    label: $t("common.fields.status"),
    width: 100,
    useSlot: true,
  },
  {
    prop: "granted_at",
    label: $t("product.detail.tenantColumns.grantedAt"),
    width: 180,
    useSlot: true,
  },
  {
    prop: "operations",
    label: $t("common.fields.operations"),
    width: 120,
    fixed: "right",
    useSlot: true,
  },
]);
const formRef = ref<FormInstance>();
const editForm = reactive<UpdateProductRequest>({
  name: "",
  enabled: true,
  timeout_secs: 30,
  cb_pro_enabled: false,
  cb_failure_threshold: 5,
  cb_success_threshold: 2,
  cb_cooldown_secs: 30,
});
const rules: FormRules = {
  name: [
    {
      required: true,
      message: $t("product.detail.validation.nameRequired"),
      trigger: "blur",
    },
    {
      min: 2,
      max: 100,
      message: $t("product.detail.validation.nameLength"),
      trigger: "blur",
    },
  ],
  timeout_secs: [
    {
      required: true,
      message: $t("product.createDialog.validation.timeoutSecsRange"),
      trigger: "blur",
    },
    {
      type: "number",
      min: 1,
      max: 300,
      message: $t("product.createDialog.validation.timeoutSecsRange"),
      trigger: "blur",
    },
  ],
  cb_failure_threshold: [
    {
      required: true,
      message: "请输入失败熔断阈值",
      trigger: "blur",
    },
  ],
  cb_success_threshold: [
    {
      required: true,
      message: "请输入成功恢复阈值",
      trigger: "blur",
    },
  ],
  cb_cooldown_secs: [
    {
      required: true,
      message: "请输入熔断冷却时间",
      trigger: "blur",
    },
  ],
};
const productCode = computed(() => route.params.id as string);
const loadAuthorizedTenants = async () => {
  try {
    await runWithLoading(tenantAccessLoading, async () => {
      const accessRes = await fetchProductTenants(productCode.value);
      let tenants: Awaited<ReturnType<typeof fetchTenantList>>["tenants"] = [];
      try {
        const tenantRes = await fetchTenantList({ page_size: 1000 });
        tenants = tenantRes.tenants;
      } catch (error) {
        logger.error(
          "Failed to load tenant directory for product access:",
          error,
        );
      }
      authorizedTenants.value = buildAuthorizedTenantRows(
        accessRes.tenants,
        tenants,
      );
    });
  } catch (error) {
    authorizedTenants.value = [];
    logger.error("Failed to load authorized tenants:", error);
  }
};
const loadData = async () => {
  await Promise.all([
    (async () => {
      try {
        await runWithLoading(loading, async () => {
          const data = await fetchProductById(productCode.value);
          product.value = data;
          syncEditForm(data);
        });
      } catch (error) {
        logger.error("Failed to load product:", error);
      }
    })(),
    loadAuthorizedTenants(),
  ]);
};
const syncEditForm = (data: Product) => {
  editForm.name = data.name;
  editForm.enabled = data.enabled;
  editForm.timeout_secs = data.timeout_secs;
  editForm.cb_pro_enabled = data.cb_pro_enabled;
  editForm.cb_failure_threshold = data.cb_failure_threshold ?? 5;
  editForm.cb_success_threshold = data.cb_success_threshold ?? 2;
  editForm.cb_cooldown_secs = data.cb_cooldown_secs ?? 30;
};
const handleEdit = () => {
  if (!product.value) return;
  syncEditForm(product.value);
  editVisible.value = true;
};
const handleSave = async () => {
  const valid = await formRef.value?.validate().catch(() => false);
  if (!valid) return;
  try {
    await runWithLoading(saving, async () => {
      const result = await updateProduct(productCode.value, toRaw(editForm));
      product.value = result;
      editVisible.value = false;
    });
  } catch (error) {
    logger.error("Failed to update product:", error);
  }
};
const handleStatusChange = async (enabled: string | number | boolean) => {
  const boolEnabled = Boolean(enabled);
  try {
    await runWithLoading(statusLoading, async () => {
      const result = await updateProduct(productCode.value, {
        enabled: boolEnabled,
      });
      product.value = result;
    });
  } catch (error) {
    logger.error("Failed to change status:", error);
  }
};
const handleDelete = async () => {
  try {
    await ElMessageBox.confirm(
      $t("product.messages.deleteProductConfirm", {
        name: product.value?.name ?? "",
      }),
      $t("common.tips"),
      {
        confirmButtonText: $t("common.confirm"),
        cancelButtonText: $t("common.cancel"),
        type: "warning",
      },
    );
    await deleteProduct(productCode.value);
    ElMessage.success($t("common.feedback.deleteSuccess"));
    router.push({
      name: "ProductAuthorization",
      query: { tab: "products" },
    });
  } catch (error) {
    if (error !== "cancel") {
      logger.error("Failed to delete product:", error);
    }
  }
};
const handleViewTenant = (tenantCode: string) => {
  router.push({ name: "TenantDetail", params: { code: tenantCode } });
};
onActivated(() => {
  loadData();
});
</script>

<style scoped lang="scss">
.product-code-value {
  display: flex;
  align-items: center;
  gap: 8px;
}

.product-descriptions {
  :deep(.el-descriptions__label) {
    width: 132px;
    min-width: 132px;
    white-space: nowrap;
  }

  :deep(.el-descriptions__content) {
    word-break: break-word;
  }
}

@media (max-width: 768px) {
  .product-descriptions {
    :deep(.el-descriptions__label) {
      width: 104px;
      min-width: 104px;
    }
  }
}

.dialog-section-title {
  margin: 0 0 12px;
  color: var(--art-gray-700);
  font-size: 13px;
  font-weight: 600;
  line-height: 1.6;
}
</style>
