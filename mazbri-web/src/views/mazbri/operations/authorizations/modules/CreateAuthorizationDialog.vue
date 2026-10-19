<template>
  <EditDialog
    :model-value="modelValue"
    :title="$t('authorization.grantDialog.title')"
    width="560px"
    :confirm-text="$t('authorization.grantDialog.confirm')"
    :saving="saving"
    @confirm="handleConfirm"
    @close="handleClose"
  >
    <ElForm
      ref="formRef"
      :model="form"
      :rules="rules"
      label-width="100px"
      class="mt-4"
    >
      <ElFormItem
        :label="$t('authorization.grantDialog.tenant')"
        prop="tenantCode"
      >
        <ElSelect
          v-model="form.tenantCode"
          :placeholder="$t('authorization.grantDialog.selectTenant')"
          style="width: 100%"
          filterable
          :loading="tenantsLoading"
        >
          <ElOption
            v-for="tenant in tenants"
            :key="tenant.tenant_id"
            :label="`${tenant.name} (${tenant.tenant_code})`"
            :value="tenant.tenant_code"
          />
        </ElSelect>
      </ElFormItem>

      <ElFormItem
        :label="$t('authorization.grantDialog.product')"
        prop="productId"
      >
        <ElSelect
          v-model="form.productId"
          :placeholder="$t('authorization.grantDialog.selectProduct')"
          style="width: 100%"
          filterable
          :loading="productsLoading"
        >
          <ElOption
            v-for="product in products"
            :key="product.id"
            :label="product.name"
            :value="product.product_id"
          />
        </ElSelect>
      </ElFormItem>

      <ElFormItem
        :label="$t('authorization.grantDialog.billingMode')"
        prop="billing_type"
      >
        <ElSelect
          v-model="form.billing_type"
          :placeholder="$t('authorization.grantDialog.selectBillingMode')"
          style="width: 100%"
        >
          <ElOption
            :label="$t('authorization.grantDialog.billingCall')"
            value="call"
          />
        </ElSelect>
      </ElFormItem>

      <ElFormItem
        :label="$t('authorization.grantDialog.unitPrice')"
        prop="unit_price"
      >
        <ElInput
          v-model="form.unit_price"
          :placeholder="$t('authorization.grantDialog.unitPricePlaceholder')"
          style="width: 100%"
        />
      </ElFormItem>

      <ElFormItem
        :label="$t('authorization.grantDialog.cbProFallback')"
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
import { $t } from "@/locales";
import { fetchTenantList } from "@/api/tenant";
import { fetchProductList } from "@/api/product";
import { grantProduct, createDefaultCBProPolicyConfig } from "@/api/tenant";
import type { Tenant } from "@/types/api/tenant";
import type { Product } from "@/types/api/product";
import EditDialog from "@/components/common/EditDialog.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { logger } from "@/utils/sys";
const props = defineProps<{
  modelValue: boolean;
}>();
const emit = defineEmits<{
  "update:modelValue": [value: boolean];
  success: [];
}>();
const formRef = ref<FormInstance>();
const saving = ref(false);
const tenantsLoading = ref(false);
const productsLoading = ref(false);
const tenants = ref<Tenant[]>([]);
const products = ref<Product[]>([]);
const form = reactive({
  tenantCode: "",
  productId: "",
  billing_type: "call",
  unit_price: "0.001",
  cb_pro_fallback: false,
});
const rules: FormRules = {
  tenantCode: [
    {
      required: true,
      message: $t("authorization.grantDialog.selectTenant"),
      trigger: "change",
    },
  ],
  productId: [
    {
      required: true,
      message: $t("authorization.grantDialog.selectProduct"),
      trigger: "change",
    },
  ],
  billing_type: [
    {
      required: true,
      message: $t("authorization.grantDialog.selectBillingMode"),
      trigger: "change",
    },
  ],
  unit_price: [
    {
      required: true,
      message: $t("authorization.grantDialog.enterUnitPrice"),
      trigger: "blur",
    },
  ],
};
const selectedProduct = computed(
  () => products.value.find((p) => p.product_id === form.productId) || null,
);
watch(selectedProduct, (val) => {
  if (!val?.cb_pro_enabled) {
    form.cb_pro_fallback = false;
  }
});
const loadOptions = async () => {
  try {
    const [tenantRes, productRes] = await Promise.all([
      runWithLoading(tenantsLoading, async () =>
        fetchTenantList({ page_size: 1000 }),
      ),
      runWithLoading(productsLoading, async () =>
        fetchProductList({ page_size: 1000 }),
      ),
    ]);
    tenants.value = tenantRes.tenants;
    products.value = productRes.products;
  } catch (error) {
    logger.error("Failed to load options for authorization:", error);
  }
};
const handleConfirm = async () => {
  if (!formRef.value) return;
  const valid = await formRef.value.validate().catch(() => false);
  if (!valid) return;
  await runWithLoading(saving, async () => {
    try {
      await grantProduct(form.tenantCode, form.productId, {
        billing_type: form.billing_type,
        unit_price: form.unit_price,
        cb_pro_fallback: form.cb_pro_fallback,
        cbpro: createDefaultCBProPolicyConfig(),
      });
      ElMessage.success($t("authorization.grantDialog.success"));
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
  form.productId = "";
  form.billing_type = "call";
  form.unit_price = "0.001";
  form.cb_pro_fallback = false;
};
watch(
  () => props.modelValue,
  (val) => {
    if (val) {
      loadOptions();
    }
  },
);
</script>
