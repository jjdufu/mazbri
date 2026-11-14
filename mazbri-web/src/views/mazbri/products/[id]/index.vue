<template>
  <div class="product-detail-shell">
    <div class="product-detail-shell__header">
      <div class="product-detail-shell__identity">
        <span
          class="product-identity__name"
          :title="product?.name || productCode"
        >
          {{ product?.name || productCode || EMPTY_PLACEHOLDER }}
        </span>
        <span
          v-if="product?.product_id || productCode"
          class="product-identity__code"
        >
          <CopyableText
            :value="product?.product_id || productCode"
            :label="t('product.fields.productId')"
          />
        </span>
        <ElTag
          :type="getStatusType(productStatusKey)"
          size="small"
          class="product-identity__tag"
        >
          {{ getStatusLabel(productStatusKey) }}
        </ElTag>
        <ElTag
          type="info"
          effect="plain"
          size="small"
          class="product-identity__tag"
        >
          {{ t("product.detail.billingModePayPerCall") }}
        </ElTag>
      </div>

      <div class="product-detail-shell__bar">
        <div class="product-detail-shell__tabs">
          <ElTabs
            :model-value="activeTab"
            class="product-detail-tabs"
            @tab-change="handleTabChange"
          >
            <ElTabPane
              v-for="item in tabOptions"
              :key="item.value"
              :label="item.label"
              :name="item.value"
            />
          </ElTabs>
        </div>
      </div>
    </div>

    <section class="product-detail-shell__content">
      <div v-show="activeTab === 'info'">
        <ElCard v-loading="loading" shadow="never" class="product-info-card">
          <div v-if="userStore.canWriteBusiness" class="product-info-actions">
            <ElButton
              v-roles="ROLES_BIZ"
              type="primary"
              @click="handleEdit"
            >
              {{ t("common.actions.modify") }}
            </ElButton>
            <ElButton
              v-roles="ROLES_BIZ"
              type="danger"
              plain
              @click="handleDelete"
            >
              <template #icon>
                <ArtSvgIcon icon="ri:delete-bin-line" />
              </template>
              {{ t("common.actions.delete") }}
            </ElButton>
          </div>

          <ElDescriptions class="product-descriptions" :column="2" border>
            <ElDescriptionsItem :label="t('product.fields.internalId')">
              {{ product?.id || EMPTY_PLACEHOLDER }}
            </ElDescriptionsItem>
            <ElDescriptionsItem :label="t('product.fields.productId')">
              <div class="product-code-value">
                <span>{{ product?.product_id || EMPTY_PLACEHOLDER }}</span>
                <ElTag size="small" effect="plain" type="info">
                  {{ t("product.detail.lockedHint") }}
                </ElTag>
              </div>
            </ElDescriptionsItem>
            <ElDescriptionsItem :label="t('product.fields.name')">
              {{ product?.name || EMPTY_PLACEHOLDER }}
            </ElDescriptionsItem>
            <ElDescriptionsItem :label="t('product.fields.enabled')">
              <ElSwitch
                :model-value="product?.enabled"
                :loading="statusLoading"
                :disabled="!userStore.canWriteBusiness"
                @change="handleStatusChange"
              />
            </ElDescriptionsItem>
            <ElDescriptionsItem :label="t('product.fields.description')" :span="2">
              {{ product?.description?.trim() || EMPTY_PLACEHOLDER }}
            </ElDescriptionsItem>
            <ElDescriptionsItem :label="t('product.fields.timeout')">
              {{
                product?.timeout_secs != null
                  ? `${product.timeout_secs} ${t("ops.units.s")}`
                  : EMPTY_PLACEHOLDER
              }}
            </ElDescriptionsItem>
            <ElDescriptionsItem :label="t('product.fields.cbProEnabled')">
              <ElTag :type="product?.cb_pro_enabled ? 'warning' : 'info'">
                {{
                  product?.cb_pro_enabled
                    ? t("common.status.enabled")
                    : t("common.status.disabled")
                }}
              </ElTag>
            </ElDescriptionsItem>
            <ElDescriptionsItem
              v-if="product?.cb_pro_enabled"
              :label="t('product.fields.cbFailureThresholdLabel')"
            >
              {{ product?.cb_failure_threshold }} {{ t("ops.units.times") }}
            </ElDescriptionsItem>
            <ElDescriptionsItem
              v-if="product?.cb_pro_enabled"
              :label="t('product.fields.cbSuccessThresholdLabel')"
            >
              {{ product?.cb_success_threshold }} {{ t("ops.units.times") }}
            </ElDescriptionsItem>
            <ElDescriptionsItem
              v-if="product?.cb_pro_enabled"
              :label="t('product.fields.cbCooldownLabel')"
              :span="2"
            >
              {{ product?.cb_cooldown_secs }} {{ t("ops.units.s") }}
            </ElDescriptionsItem>
            <ElDescriptionsItem :label="t('common.fields.createdAt')">
              {{ formatDate(product?.created_at || "") }}
            </ElDescriptionsItem>
            <ElDescriptionsItem :label="t('common.fields.updatedAt')">
              {{ formatDate(product?.updated_at || "") }}
            </ElDescriptionsItem>
          </ElDescriptions>
        </ElCard>
      </div>

      <div
        v-if="product?.id && isTabLoaded('routes')"
        v-show="activeTab === 'routes'"
      >
        <UserRouteGovernance
          embedded
          :initial-product-id="product.id"
          :lock-product="true"
        />
      </div>

      <div
        v-if="isTabLoaded('tenants')"
        v-show="activeTab === 'tenants'"
        ref="tenantsPanelRef"
      >
        <ResourceTableShell
          v-model:columns="authorizedTenantColumns"
          :title="t('product.detail.authorizedTenantsTitle')"
          :loading="tenantAccessLoading"
          :data="authorizedTenants"
          :empty-text="tenantEmptyText"
          :pagination="tenantArtPagination"
          :fit="true"
          table-layout="auto"
          stripe
          border
          @refresh="loadAuthorizedTenants"
          @pagination:size-change="handleTenantPageSizeChange"
          @pagination:current-change="handleTenantPageChange"
        >
          <template #headerLeft>
            <ListQueryBar
              v-model:q="tenantSearchKeyword"
              :enable-filters="false"
              :placeholder="t('product.detail.searchTenantsPlaceholder')"
              @search="handleTenantSearch"
            />
          </template>

          <template v-if="userStore.canWriteBusiness" #actions>
            <ElButton
              v-roles="ROLES_BIZ"
              type="primary"
              @click="grantDialogVisible = true"
            >
              <template #icon><ArtSvgIcon icon="ri:add-line" /></template>
              {{ t("product.detail.addAuthorization") }}
            </ElButton>
          </template>

          <template #tenant_name="{ row }">
            {{ row.tenant_name || row.tenant_code || EMPTY_PLACEHOLDER }}
          </template>
          <template #tenant_id="{ row }">
            <CopyableText
              :value="row.tenant_id"
              :label="t('product.detail.tenantColumns.tenantId')"
              compact
            />
          </template>
          <template #tenant_code="{ row }">
            <CopyableText
              :value="row.tenant_code"
              :label="t('product.detail.tenantColumns.tenantCode')"
            />
          </template>
          <template #tenant_enabled="{ row }">
            <StatusIndicator
              :type="getStatusType(row.tenant_enabled ? 'enabled' : 'disabled')"
            >
              {{ getStatusLabel(row.tenant_enabled ? "enabled" : "disabled") }}
            </StatusIndicator>
          </template>
          <template #created_at="{ row }">
            {{ formatDate(row.created_at) }}
          </template>
          <template #operations="{ row }">
            <TableActionRow>
              <ElButton
                link
                type="primary"
                @click="handleViewTenant(row.tenant_code)"
              >
                {{ t("common.actions.details") }}
              </ElButton>
              <ElPopconfirm
                v-if="userStore.canWriteBusiness"
                :title="t('authorization.list.revokeConfirm')"
                @confirm="handleRevokeTenant(row)"
              >
                <template #reference>
                  <ElButton v-roles="ROLES_BIZ" link type="danger">
                    {{ t("authorization.list.actions.revoke") }}
                  </ElButton>
                </template>
              </ElPopconfirm>
            </TableActionRow>
          </template>
        </ResourceTableShell>
      </div>
    </section>

    <EditDialog
      v-model="editVisible"
      :title="t('product.detail.editTitle')"
      width="560px"
      :saving="saving"
      @confirm="handleSave"
    >
      <ElAlert
        :title="t('product.detail.lockedAlert')"
        type="info"
        :closable="false"
        class="mb-4"
      />
      <ElForm
        ref="formRef"
        :model="editForm"
        :rules="rules"
        :label-width="editFormLabelWidth"
      >
        <div class="dialog-section">
          <p class="dialog-section-title">
            {{ t("product.detail.basicInfoTitle") }}
          </p>
          <ElFormItem :label="t('product.fields.name')" prop="name">
            <ElInput v-model="editForm.name" />
          </ElFormItem>
          <ElFormItem :label="t('product.fields.description')" prop="description">
            <ElInput
              v-model="editForm.description"
              type="textarea"
              :rows="2"
              :placeholder="t('product.createDialog.placeholders.description')"
            />
          </ElFormItem>
        </div>

        <div class="dialog-section mt-4">
          <p class="dialog-section-title">
            {{ t("product.detail.advancedInfoTitle") }}
          </p>
          <ElFormItem :label="t('product.fields.timeout')" prop="timeout_secs">
            <div class="unit-input-row">
              <ElInputNumber
                v-model="editForm.timeout_secs"
                :min="1"
                :max="300"
                controls-position="right"
                class="unit-input-number"
              />
              <span class="field-unit">{{ t("ops.units.s") }}</span>
            </div>
          </ElFormItem>
          <ElFormItem :label="t('product.fields.enabled')">
            <ElSwitch v-model="editForm.enabled" />
          </ElFormItem>
          <ElFormItem
            :label="t('product.fields.cbProEnabled')"
            prop="cb_pro_enabled"
          >
            <ElSwitch v-model="editForm.cb_pro_enabled" />
          </ElFormItem>
          <ElFormItem
            v-if="editForm.cb_pro_enabled"
            :label="t('product.fields.cbFailureThresholdLabel')"
            prop="cb_failure_threshold"
          >
            <div class="unit-input-row">
              <ElInputNumber
                v-model="editForm.cb_failure_threshold"
                :min="1"
                controls-position="right"
                class="unit-input-number"
              />
              <span class="field-unit">{{ t("ops.units.times") }}</span>
            </div>
          </ElFormItem>
          <ElFormItem
            v-if="editForm.cb_pro_enabled"
            :label="t('product.fields.cbSuccessThresholdLabel')"
            prop="cb_success_threshold"
          >
            <div class="unit-input-row">
              <ElInputNumber
                v-model="editForm.cb_success_threshold"
                :min="1"
                controls-position="right"
                class="unit-input-number"
              />
              <span class="field-unit">{{ t("ops.units.times") }}</span>
            </div>
          </ElFormItem>
          <ElFormItem
            v-if="editForm.cb_pro_enabled"
            :label="t('product.fields.cbCooldownLabel')"
            prop="cb_cooldown_secs"
          >
            <div class="unit-input-row">
              <ElInputNumber
                v-model="editForm.cb_cooldown_secs"
                :min="1"
                controls-position="right"
                class="unit-input-number"
              />
              <span class="field-unit">{{ t("ops.units.s") }}</span>
            </div>
          </ElFormItem>
        </div>
      </ElForm>
    </EditDialog>

    <CreateAuthorizationDialog
      v-model="grantDialogVisible"
      :locked-product="product"
      @success="handleGrantSuccess"
    />
  </div>
</template>

<script setup lang="ts">
import CopyableText from "@/components/common/copyable-text/index.vue";
import type { FormInstance, FormRules } from "element-plus";
import { ElMessageBox, ElMessage } from "element-plus";
import { useI18n } from "vue-i18n";
import UserRouteGovernance from "@/views/mazbri/operations/user-routes/index.vue";
import CreateAuthorizationDialog from "@/views/mazbri/operations/authorizations/modules/CreateAuthorizationDialog.vue";
import { ROLES_BIZ } from "@/utils/ops/roles";
import type {
  Product,
  ProductTenantAccess,
  UpdateProductRequest,
} from "@/types/api/product";
import {
  fetchProductById,
  fetchProductTenants,
  updateProduct,
  deleteProduct,
} from "@/api/product";
import { revokeProduct } from "@/api/tenant";
import EditDialog from "@/components/common/EditDialog.vue";
import ListQueryBar from "@/components/common/list-query-bar/index.vue";
import StatusIndicator from "@/components/common/StatusIndicator.vue";
import ResourceTableShell from "@/components/common/resource-table-shell/index.vue";
import { useAdaptiveTableColumns } from "@/composables/useAdaptiveTableColumns";
import { runWithLoading } from "@/composables/useAsyncState";
import { useTabRouteSync } from "@/composables/useTabRouteSync";
import { useUserStore } from "@/store/modules/user";
import { logger } from "@/utils/sys";
import { getStatusType, getStatusLabel } from "@/utils/display-labels";
import { EMPTY_PLACEHOLDER, formatDateTime as formatDate } from "@/utils/format";
import type { ColumnOption, PaginationConfig } from "@/types/component";

defineOptions({ name: "ProductDetail" });

type ProductDetailTab = "info" | "routes" | "tenants";

const router = useRouter();
const route = useRoute();
const { t, locale } = useI18n();
const userStore = useUserStore();

const loading = ref(false);
const saving = ref(false);
const statusLoading = ref(false);
const editVisible = ref(false);
const grantDialogVisible = ref(false);
const product = ref<Product | null>(null);

const editFormLabelWidth = computed(() =>
  String(locale.value).startsWith("en") ? "156px" : "124px",
);

const productStatusKey = computed(() =>
  product.value?.enabled === false ? "disabled" : "enabled",
);

const { activeTab, handleTabChange } = useTabRouteSync<ProductDetailTab>({
  defaultValue: "info",
  allowedValues: ["info", "routes", "tenants"],
});

const tabOptions = computed(() => [
  { label: t("product.detail.tabs.info"), value: "info" },
  { label: t("product.detail.tabs.routes"), value: "routes" },
  { label: t("product.detail.tabs.tenants"), value: "tenants" },
]);

const loadedTabs = ref<Set<ProductDetailTab>>(new Set([activeTab.value]));
const isTabLoaded = (tab: ProductDetailTab) => loadedTabs.value.has(tab);

const tenantsPanelRef = ref<HTMLElement | null>(null);
const tenantAccessLoading = ref(false);
const tenantSearchKeyword = ref("");
const authorizedTenants = ref<ProductTenantAccess[]>([]);
const tenantPagination = reactive({
  page: 1,
  page_size: 10,
  total: 0,
});
const tenantsFetchedForCode = ref("");

const tenantArtPagination = computed<PaginationConfig>(() => ({
  current: tenantPagination.page,
  size: tenantPagination.page_size,
  total: tenantPagination.total,
}));

const tenantEmptyText = computed(() =>
  tenantSearchKeyword.value.trim()
    ? t("product.detail.noMatchingTenants")
    : t("product.detail.noAuthorizedTenants"),
);

const authorizedTenantColumns = ref<ColumnOption<ProductTenantAccess>[]>([]);
const baseAuthorizedTenantColumns = computed<
  ColumnOption<ProductTenantAccess>[]
>(() => {
  const isEn = String(locale.value).startsWith("en");
  return [
    {
      prop: "tenant_name",
      label: t("product.detail.tenantColumns.name"),
      minWidth: 220,
      maxWidth: 280,
      showOverflowTooltip: true,
      useSlot: true,
    },
    {
      prop: "tenant_id",
      useSlot: true,
      label: t("product.detail.tenantColumns.tenantId"),
      minWidth: 160,
    },
    {
      prop: "tenant_code",
      useSlot: true,
      label: t("product.detail.tenantColumns.tenantCode"),
      width: 130,
    },
    {
      prop: "tenant_enabled",
      label: t("common.fields.status"),
      width: 100,
      useSlot: true,
    },
    {
      prop: "created_at",
      label: t("product.detail.tenantColumns.grantedAt"),
      width: 180,
      useSlot: true,
    },
    {
      prop: "operations",
      label: t("common.fields.operations"),
      width: userStore.canWriteBusiness ? (isEn ? 168 : 140) : 96,
      fixed: "right",
      useSlot: true,
    },
  ];
});

useAdaptiveTableColumns({
  columns: authorizedTenantColumns,
  containerRef: tenantsPanelRef,
  rows: authorizedTenants,
  baseColumns: baseAuthorizedTenantColumns,
  getCellText: ({ columnKey, row }) => {
    switch (columnKey) {
      case "tenant_name":
        return row.tenant_name || row.tenant_code || EMPTY_PLACEHOLDER;
      case "tenant_id":
        return row.tenant_id || EMPTY_PLACEHOLDER;
      case "tenant_code":
        return row.tenant_code || EMPTY_PLACEHOLDER;
      case "tenant_enabled":
        return getStatusLabel(row.tenant_enabled ? "enabled" : "disabled");
      case "created_at":
        return formatDate(row.created_at);
      case "operations":
        return userStore.canWriteBusiness
          ? `${t("common.actions.details")} ${t("authorization.list.actions.revoke")}`
          : t("common.actions.details");
      default:
        return row.tenant_name || EMPTY_PLACEHOLDER;
    }
  },
});

const formRef = ref<FormInstance>();
const editForm = reactive<UpdateProductRequest>({
  name: "",
  description: "",
  enabled: true,
  timeout_secs: 30,
  cb_pro_enabled: false,
  cb_failure_threshold: 5,
  cb_success_threshold: 2,
  cb_cooldown_secs: 30,
});

const rules = computed<FormRules>(() => ({
  name: [
    {
      required: true,
      message: t("product.detail.validation.nameRequired"),
      trigger: "blur",
    },
    {
      min: 2,
      max: 100,
      message: t("product.detail.validation.nameLength"),
      trigger: "blur",
    },
  ],
  timeout_secs: [
    {
      required: true,
      message: t("product.createDialog.validation.timeoutSecsRange"),
      trigger: "blur",
    },
    {
      type: "number",
      min: 1,
      max: 300,
      message: t("product.createDialog.validation.timeoutSecsRange"),
      trigger: "blur",
    },
  ],
  cb_failure_threshold: [
    {
      required: true,
      message: t("product.detail.validation.cbFailureThresholdRequired"),
      trigger: "blur",
    },
  ],
  cb_success_threshold: [
    {
      required: true,
      message: t("product.detail.validation.cbSuccessThresholdRequired"),
      trigger: "blur",
    },
  ],
  cb_cooldown_secs: [
    {
      required: true,
      message: t("product.detail.validation.cbCooldownSecsRequired"),
      trigger: "blur",
    },
  ],
}));

const productCode = computed(() => String(route.params.id || "").trim());

const loadProduct = async () => {
  const code = productCode.value;
  if (!code) {
    product.value = null;
    return;
  }
  try {
    await runWithLoading(loading, async () => {
      const data = await fetchProductById(code);
      product.value = data;
      syncEditForm(data);
    });
  } catch (error) {
    logger.error("Failed to load product:", error);
  }
};

let tenantAbortController: AbortController | null = null;

const loadAuthorizedTenants = async () => {
  const code = productCode.value;
  if (!code) {
    authorizedTenants.value = [];
    tenantPagination.total = 0;
    return;
  }
  tenantAbortController?.abort();
  const controller = new AbortController();
  tenantAbortController = controller;
  tenantAccessLoading.value = true;
  try {
    const keyword = tenantSearchKeyword.value.trim();
    const accessRes = await fetchProductTenants(
      code,
      {
        page: tenantPagination.page,
        page_size: tenantPagination.page_size,
        q: keyword || undefined,
      },
      controller.signal,
    );
    if (controller.signal.aborted) return;
    authorizedTenants.value = accessRes.tenants;
    tenantPagination.total = accessRes.pagination.total;
    tenantsFetchedForCode.value = code;
  } catch (error) {
    if (controller.signal.aborted) return;
    authorizedTenants.value = [];
    tenantPagination.total = 0;
    logger.error("Failed to load authorized tenants:", error);
  } finally {
    if (tenantAbortController === controller) {
      tenantAccessLoading.value = false;
      tenantAbortController = null;
    }
  }
};

onBeforeUnmount(() => {
  tenantAbortController?.abort();
});

const handleTenantSearch = () => {
  tenantPagination.page = 1;
  void loadAuthorizedTenants();
};

const handleTenantPageSizeChange = (size: number) => {
  tenantPagination.page_size = size;
  tenantPagination.page = 1;
  void loadAuthorizedTenants();
};

const handleTenantPageChange = (page: number) => {
  tenantPagination.page = page;
  void loadAuthorizedTenants();
};

const handleGrantSuccess = () => {
  tenantPagination.page = 1;
  void loadAuthorizedTenants();
};

const handleRevokeTenant = async (row: ProductTenantAccess) => {
  try {
    await revokeProduct(row.tenant_code, productCode.value);
    ElMessage.success(t("authorization.list.revokeSuccess"));
    if (authorizedTenants.value.length === 1 && tenantPagination.page > 1) {
      tenantPagination.page -= 1;
    }
    await loadAuthorizedTenants();
  } catch (error) {
    logger.error("Failed to revoke tenant product access:", error);
  }
};

const syncEditForm = (data: Product) => {
  editForm.name = data.name;
  editForm.description = data.description ?? "";
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
      t("product.messages.deleteProductConfirm", {
        name: product.value?.name ?? "",
      }),
      t("common.tips"),
      {
        confirmButtonText: t("common.confirm"),
        cancelButtonText: t("common.cancel"),
        type: "warning",
      },
    );
    await deleteProduct(productCode.value);
    ElMessage.success(t("common.feedback.deleteSuccess"));
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

watch(
  activeTab,
  (tab) => {
    loadedTabs.value.add(tab);
    if (tab === "tenants" && tenantsFetchedForCode.value !== productCode.value) {
      void loadAuthorizedTenants();
    }
  },
  { immediate: true },
);

watch(
  () => route.params.id,
  (nextId, prevId) => {
    if (nextId && nextId !== prevId) {
      tenantSearchKeyword.value = "";
      tenantPagination.page = 1;
      tenantsFetchedForCode.value = "";
      loadedTabs.value = new Set([activeTab.value]);
      void loadProduct();
      if (activeTab.value === "tenants") {
        void loadAuthorizedTenants();
      }
    }
  },
);

onMounted(() => {
  void loadProduct();
});
</script>

<style scoped lang="scss">
.product-detail-shell {
  display: flex;
  flex-direction: column;
  gap: var(--ops-section-gap, 16px);
  padding: var(--ops-page-padding, 16px);
  min-width: 0;
  box-sizing: border-box;
}

.product-detail-shell__header {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}

.product-detail-shell__identity {
  display: flex;
  flex-wrap: nowrap;
  gap: 10px;
  align-items: center;
  min-width: 0;
  min-height: 24px;
  max-height: 28px;
}

.product-identity__name {
  font-size: 15px;
  font-weight: 600;
  line-height: 22px;
  color: var(--el-text-color-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 360px;
}

.product-identity__code {
  display: inline-flex;
  align-items: center;
  flex-shrink: 0;
  padding: 1px 8px;
  border-radius: var(--art-form-control-radius-sm, 4px);
  background: var(--el-fill-color-light);
  color: var(--el-text-color-regular);
  line-height: 20px;
}

.product-identity__tag {
  flex-shrink: 0;
}

.product-detail-shell__bar {
  display: flex;
  align-items: center;
  min-width: 0;
  min-height: 36px;
}

.product-detail-shell__tabs {
  flex: 1 1 auto;
  min-width: 0;

  :deep(.el-tabs__header) {
    margin: 0 !important;
  }

  :deep(.el-tabs__content) {
    display: none !important;
  }
}

.product-detail-shell__content {
  display: flex;
  flex-direction: column;
  gap: var(--ops-section-gap, 16px);
  min-width: 0;
}

.product-info-actions {
  display: flex;
  justify-content: flex-end;
  align-items: center;
  gap: 8px;
  margin-bottom: 12px;
}

.product-code-value {
  display: flex;
  align-items: center;
  gap: 8px;
}

.product-descriptions {
  :deep(.el-descriptions__label) {
    width: 148px;
    min-width: 148px;
    white-space: nowrap;
  }

  :deep(.el-descriptions__content) {
    word-break: break-word;
  }
}

@media (max-width: 768px) {
  .product-descriptions {
    :deep(.el-descriptions__label) {
      width: 112px;
      min-width: 112px;
    }
  }
}

.dialog-section-title {
  margin: 0 0 12px;
  color: var(--el-text-color-primary);
  font-size: 13px;
  font-weight: 600;
  line-height: 1.6;
}

.unit-input-row {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
}

.unit-input-number {
  flex: 1;
  min-width: 0;
}

.field-unit {
  flex-shrink: 0;
  font-size: 13px;
  color: var(--el-text-color-secondary);
}
</style>
