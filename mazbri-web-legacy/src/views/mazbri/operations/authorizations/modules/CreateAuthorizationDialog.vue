<template>
  <EditDialog
    :model-value="modelValue"
    :title="t('authorization.grantDialog.title')"
    width="560px"
    :confirm-text="t('authorization.grantDialog.confirm')"
    :saving="saving"
    @confirm="handleConfirm"
    @close="handleClose"
  >
    <ElForm
      ref="formRef"
      :model="form"
      :rules="rules"
      label-width="108px"
      class="mt-4"
    >
      <ElFormItem
        :label="t('authorization.grantDialog.tenant')"
        prop="tenantCode"
      >
        <ElSelect
          v-model="form.tenantCode"
          :placeholder="t('authorization.grantDialog.searchTenantPlaceholder')"
          style="width: 100%"
          filterable
          remote
          reserve-keyword
          :remote-method="searchTenants"
          :loading="tenantsLoading"
        >
          <ElOption
            v-for="tenant in tenants"
            :key="tenant.tenant_code"
            :label="`${tenant.name} (${tenant.tenant_code})`"
            :value="tenant.tenant_code"
          />
        </ElSelect>
      </ElFormItem>

      <ElFormItem
        :label="t('authorization.grantDialog.product')"
        prop="productId"
      >
        <ElInput
          v-if="lockedProduct"
          :model-value="`${lockedProduct.name} (${lockedProduct.product_id})`"
          disabled
        />
        <ElSelect
          v-else
          v-model="form.productId"
          :placeholder="t('authorization.grantDialog.searchProductPlaceholder')"
          style="width: 100%"
          filterable
          remote
          reserve-keyword
          :remote-method="searchProducts"
          :loading="productsLoading"
        >
          <ElOption
            v-for="product in products"
            :key="product.id"
            :label="`${product.name} (${product.product_id})`"
            :value="product.product_id"
          />
        </ElSelect>
      </ElFormItem>

      <ElFormItem
        :label="t('authorization.grantDialog.unitPrice')"
        prop="unit_price"
      >
        <ElInput
          v-model="form.unit_price"
          :placeholder="t('authorization.grantDialog.unitPricePlaceholder')"
          style="width: 100%"
        />
      </ElFormItem>

      <ElFormItem
        :label="t('authorization.grantDialog.cbProFallback')"
        prop="cb_pro_fallback"
      >
        <ElSwitch
          v-model="form.cb_pro_fallback"
          :disabled="!selectedProduct?.cb_pro_enabled"
        />
      </ElFormItem>
    </ElForm>
  </EditDialog>
</template>

<script setup lang="ts">
import type { FormInstance, FormRules } from "element-plus";
import { ElMessage } from "element-plus";
import { useI18n } from "vue-i18n";
import {
  fetchTenantList,
  grantProduct,
  createDefaultCBProPolicyConfig,
} from "@/api/tenant";
import { fetchProductList } from "@/api/product";
import type { Tenant } from "@/types/api/tenant";
import type { Product } from "@/types/api/product";
import EditDialog from "@/components/common/EditDialog.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { logger } from "@/utils/sys";

const props = withDefaults(
  defineProps<{
    modelValue: boolean;
    lockedProduct?: Product | null;
  }>(),
  {
    lockedProduct: null,
  },
);

const emit = defineEmits<{
  "update:modelValue": [value: boolean];
  success: [];
}>();

const { t } = useI18n();
const formRef = ref<FormInstance>();
const saving = ref(false);
const tenantsLoading = ref(false);
const productsLoading = ref(false);
const tenants = ref<Tenant[]>([]);
const products = ref<Product[]>([]);

const form = reactive({
  tenantCode: "",
  productId: "",
  unit_price: "0.001",
  cb_pro_fallback: false,
});

const rules = computed<FormRules>(() => ({
  tenantCode: [
    {
      required: true,
      message: t("authorization.grantDialog.selectTenant"),
      trigger: "change",
    },
  ],
  productId: [
    {
      required: true,
      message: t("authorization.grantDialog.selectProduct"),
      trigger: "change",
    },
  ],
  unit_price: [
    {
      required: true,
      message: t("authorization.grantDialog.enterUnitPrice"),
      trigger: "blur",
    },
  ],
}));

const selectedProduct = computed(
  () =>
    props.lockedProduct ||
    products.value.find((p) => p.product_id === form.productId) ||
    null,
);

watch(selectedProduct, (val) => {
  if (!val?.cb_pro_enabled) {
    form.cb_pro_fallback = false;
  }
});

const searchTenants = async (query = "") => {
  try {
    await runWithLoading(tenantsLoading, async () => {
      const keyword = query.trim();
      const res = await fetchTenantList({
        page: 1,
        page_size: 20,
        q: keyword || undefined,
      });
      tenants.value = res.tenants;
    });
  } catch (error) {
    logger.error("Failed to search tenants for authorization:", error);
  }
};

const searchProducts = async (query = "") => {
  if (props.lockedProduct) {
    products.value = [props.lockedProduct];
    return;
  }
  try {
    await runWithLoading(productsLoading, async () => {
      const keyword = query.trim();
      const res = await fetchProductList({
        page: 1,
        page_size: 20,
        q: keyword || undefined,
      });
      products.value = res.products;
    });
  } catch (error) {
    logger.error("Failed to search products for authorization:", error);
  }
};

const handleConfirm = async () => {
  if (!formRef.value) return;
  const valid = await formRef.value.validate().catch(() => false);
  if (!valid) return;
  await runWithLoading(saving, async () => {
    try {
      await grantProduct(
        form.tenantCode,
        form.productId,
        {
          unit_price: form.unit_price,
          cb_pro_fallback: form.cb_pro_fallback,
          cbpro: createDefaultCBProPolicyConfig(),
        },
        { showSuccessMessage: false },
      );
      ElMessage.success(t("authorization.grantDialog.success"));
      emit("success");
      handleClose();
    } catch (error) {
      logger.error("Failed to grant authorization:", error);
    }
  });
};

const handleClose = () => {
  emit("update:modelValue", false);
  formRef.value?.resetFields();
  form.tenantCode = "";
  form.productId = props.lockedProduct?.product_id || "";
  form.unit_price = "0.001";
  form.cb_pro_fallback = false;
};

watch(
  () => props.modelValue,
  (val) => {
    if (val) {
      form.productId = props.lockedProduct?.product_id || "";
      void searchTenants("");
      if (!props.lockedProduct) {
        void searchProducts("");
      }
    }
  },
);
</script>
