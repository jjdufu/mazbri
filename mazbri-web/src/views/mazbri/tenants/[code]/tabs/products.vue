<template>
  <div class="products-tab">
    <ResourceTableShell
      v-model:columns="productColumns"
      :title="$t('tenant.products.title')"
      :loading="loading"
      :data="products"
      :show-toolbar="false"
      table-class="surface-panel-table tenant-product-table"
      stripe
      border
    >
      <template #actions>
        <ElButton
          link
          type="primary"
          class="mr-3"
          @click="
            router.push({
              name: 'ProductAuthorization',
              query: { tab: 'authorizations' },
            })
          "
        >
          {{ $t("tenant.products.manageGlobalAuth") }}
        </ElButton>
        <ElButton
          v-if="productActions.grant"
          v-roles="['R_ADMIN', 'R_OPERATOR']"
          type="primary"
          @click="handleGrant"
        >
          {{ $t("tenant.products.grantProduct") }}
        </ElButton>
      </template>

      <template #description="{ row }">
        {{ row.description || "-" }}
      </template>
      <template #granted_at="{ row }">
        {{ formatDate(row.granted_at) }}
      </template>
      <template #billing_type="{ row }">
        {{ getBillingTypeLabel(row.config.billing_type) }}
      </template>
      <template #unit_price="{ row }">
        {{ row.config.unit_price }} {{ $t("common.currencies.cny") }}
      </template>
      <template #cb_pro_fallback="{ row }">
        <ElTag
          v-if="row.cb_pro_enabled"
          :type="row.config.cb_pro_fallback ? 'warning' : 'info'"
        >
          {{
            row.config.cb_pro_fallback
              ? $t("common.status.enabled")
              : $t("common.status.disabled")
          }}
        </ElTag>
        <span v-else>-</span>
      </template>
      <template #operations="{ row }">
        <TableActionRow>
          <ElButton
            v-roles="['R_ADMIN', 'R_OPERATOR']"
            link
            type="primary"
            @click="handleConfigure(row)"
          >
            {{ $t("common.actions.modify") }}
          </ElButton>
          <ElButton
            v-if="productActions.revoke"
            v-roles="['R_ADMIN', 'R_OPERATOR']"
            link
            type="danger"
            @click="handleRevoke(row)"
          >
            {{ $t("common.actions.delete") }}
          </ElButton>
        </TableActionRow>
      </template>
    </ResourceTableShell>

    <EditDialog
      v-model="grantVisible"
      :title="$t('tenant.products.grantDialog.title')"
      width="560px"
      :confirm-text="$t('tenant.products.grantDialog.confirm')"
      :saving="granting"
      @confirm="handleSubmitGrant"
    >
      <ElForm :model="grantForm" label-width="100px">
        <ElFormItem :label="$t('tenant.products.grantDialog.product')">
          <ElSelect
            v-model="grantForm.productId"
            :placeholder="$t('tenant.products.grantDialog.productPlaceholder')"
            style="width: 100%"
            filterable
          >
            <ElOption
              v-for="product in availableProducts"
              :key="product.id"
              :label="product.name"
              :value="product.product_id"
            />
          </ElSelect>
        </ElFormItem>
        <ElFormItem :label="$t('tenant.products.grantDialog.billingMode')">
          <ElSelect
            v-model="grantForm.billing_type"
            :placeholder="
              $t('tenant.products.grantDialog.billingModePlaceholder')
            "
            style="width: 100%"
          >
            <ElOption
              :label="$t('authorization.grantDialog.billingCall')"
              value="call"
            />
          </ElSelect>
        </ElFormItem>
        <ElFormItem :label="$t('tenant.products.grantDialog.unitPrice')">
          <ElInput
            v-model="grantForm.unit_price"
            :placeholder="
              $t('tenant.products.grantDialog.unitPricePlaceholder')
            "
            style="width: 100%"
          />
        </ElFormItem>
        <ElFormItem :label="$t('tenant.products.grantDialog.cbProFallback')">
          <ElSwitch
            v-model="grantForm.cb_pro_fallback"
            :disabled="!selectedGrantProduct?.cb_pro_enabled"
          />
        </ElFormItem>
      </ElForm>
    </EditDialog>

    <ChannelConfigDrawer
      v-model="configDrawerVisible"
      :tenant-code="resolvedTenantCode"
      :product="selectedProduct"
      @updated="loadData"
    />
  </div>
</template>

<script setup lang="ts">
import { ElMessageBox, ElMessage } from "element-plus";
import { $t } from "@/locales";
import type { TenantProduct } from "@/types/api/tenant";
import type { Product } from "@/types/api/product";
import {
  fetchTenantProducts,
  grantProduct,
  revokeProduct,
  createDefaultCBProPolicyConfig,
} from "@/api/tenant";
import { fetchProductList } from "@/api/product";
import EditDialog from "@/components/common/EditDialog.vue";
import ChannelConfigDrawer from "@/components/common/channel-config-dialog/index.vue";
import ResourceTableShell from "@/components/common/resource-table-shell/index.vue";
import { logger } from "@/utils/sys";
import { formatDateTime as formatDate } from "@/utils/format";
import type { ColumnOption } from "@/types/component";
import { isValidTenantCode, normalizeTenantCode } from "@/utils/tenant-code";
defineOptions({ name: "ProductsTab" });
const props = defineProps<{
  tenantCode: string;
}>();
const router = useRouter();
const loading = ref(false);
const granting = ref(false);
const products = ref<TenantProduct[]>([]);
const productActions = ref<{
  grant: boolean;
  revoke: boolean;
}>({
  grant: true,
  revoke: true,
});
const availableProducts = ref<Product[]>([]);
const grantVisible = ref(false);
const configDrawerVisible = ref(false);
const selectedProduct = ref<TenantProduct | null>(null);
const grantForm = reactive({
  productId: "",
  billing_type: "call",
  unit_price: "0.001",
  cb_pro_fallback: false,
});
const productColumns = ref<ColumnOption<TenantProduct>[]>([
  {
    prop: "product_id",
    label: $t("tenant.products.columns.productId"),
    width: 180,
  },
  { prop: "name", label: $t("tenant.products.columns.name"), minWidth: 100 },
  {
    prop: "description",
    label: $t("tenant.products.columns.description"),
    minWidth: 112,
    useSlot: true,
  },
  {
    prop: "granted_at",
    label: $t("tenant.products.columns.grantedAt"),
    width: 170,
    useSlot: true,
  },
  {
    prop: "billing_type",
    label: $t("tenant.products.columns.billingType"),
    width: 120,
    useSlot: true,
  },
  {
    prop: "unit_price",
    label: $t("tenant.products.columns.unitPrice"),
    width: 110,
    useSlot: true,
  },
  {
    prop: "cb_pro_fallback",
    label: $t("tenant.products.columns.cbProFallback"),
    width: 110,
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
const resolvedTenantCode = computed(
  () => normalizeTenantCode(props.tenantCode) ?? "",
);
const selectedGrantProduct = computed(
  () =>
    availableProducts.value.find(
      (product) => product.product_id === grantForm.productId,
    ) || null,
);
const getBillingTypeLabel = (type?: string) => {
  const labels: Record<string, string> = {
    call: $t("authorization.grantDialog.billingCall"),
  };
  return labels[type || ""] || type || "-";
};
const resetGrantForm = () => {
  grantForm.productId = "";
  grantForm.billing_type = "call";
  grantForm.unit_price = "0.001";
  grantForm.cb_pro_fallback = false;
};
watch(selectedGrantProduct, (product) => {
  if (!product?.cb_pro_enabled) {
    grantForm.cb_pro_fallback = false;
  }
});
const loadData = async () => {
  const code = resolvedTenantCode.value;
  if (!isValidTenantCode(code)) {
    products.value = [];
    logger.warn("ProductsTab: skip load; invalid tenantCode", props.tenantCode);
    return;
  }
  loading.value = true;
  try {
    const res = await fetchTenantProducts(code);
    products.value = res.products;
    if (res.actions) {
      productActions.value = res.actions;
    }
  } catch (error) {
    logger.error("Failed to load tenant products:", error);
  } finally {
    loading.value = false;
  }
};
const loadAvailableProducts = async () => {
  try {
    const res = await fetchProductList({ page_size: 100 });
    const grantedIds = new Set(products.value.map((p) => p.product_id));
    availableProducts.value = res.products.filter(
      (p) => !grantedIds.has(p.product_id),
    );
  } catch (error) {
    logger.error("Failed to load products:", error);
  }
};
const handleGrant = () => {
  if (!isValidTenantCode(resolvedTenantCode.value)) {
    ElMessage.warning($t("tenant.validation.tenantCodeRequired"));
    return;
  }
  resetGrantForm();
  loadAvailableProducts();
  grantVisible.value = true;
};
const handleConfigure = (row: TenantProduct) => {
  if (!isValidTenantCode(resolvedTenantCode.value)) {
    ElMessage.warning($t("tenant.validation.tenantCodeRequired"));
    return;
  }
  selectedProduct.value = row;
  configDrawerVisible.value = true;
};
const handleSubmitGrant = async () => {
  const code = resolvedTenantCode.value;
  if (!isValidTenantCode(code)) {
    ElMessage.warning($t("tenant.validation.tenantCodeRequired"));
    return;
  }
  if (!grantForm.productId) {
    ElMessage.warning($t("tenant.messages.selectProductToGrant"));
    return;
  }
  granting.value = true;
  try {
    await grantProduct(code, grantForm.productId, {
      billing_type: grantForm.billing_type,
      unit_price: grantForm.unit_price,
      cb_pro_fallback: selectedGrantProduct.value?.cb_pro_enabled
        ? grantForm.cb_pro_fallback
        : false,


      cbpro: createDefaultCBProPolicyConfig(),
    });
    ElMessage.success($t("common.feedback.authorizationGranted"));
    grantVisible.value = false;
    loadData();
  } catch (error) {
    logger.error("Failed to grant product:", error);
  } finally {
    granting.value = false;
  }
};
const handleRevoke = async (row: TenantProduct) => {
  const code = resolvedTenantCode.value;
  if (!isValidTenantCode(code)) {
    ElMessage.warning($t("tenant.validation.tenantCodeRequired"));
    return;
  }
  try {
    await ElMessageBox.confirm(
      $t("tenant.messages.revokeProductConfirm", { name: row.name }),
      $t("common.tips"),
      {
        confirmButtonText: $t("common.confirm"),
        cancelButtonText: $t("common.cancel"),
        type: "warning",
      },
    );
    await revokeProduct(code, row.product_id);
    ElMessage.success($t("common.feedback.authorizationRevoked"));
    loadData();
  } catch (error) {
    if (error !== "cancel") {
      logger.error("Failed to revoke product:", error);
    }
  }
};

watch(
  resolvedTenantCode,
  (code, prev) => {
    if (code && code !== prev) {
      loadData();
    }
  },
  { immediate: true },
);
</script>

<style scoped lang="scss">
.tenant-product-table {
  :deep(.el-table__cell) {
    padding-top: 10px;
    padding-bottom: 10px;
  }
}

</style>
