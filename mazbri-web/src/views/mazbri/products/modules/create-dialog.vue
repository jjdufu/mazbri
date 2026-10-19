<template>
  <EditDialog
    v-model="visible"
    :title="$t('product.createDialog.title')"
    width="560px"
    :confirm-text="$t('common.confirm')"
    :saving="submitting"
    @confirm="handleSubmit"
  >
    <ElForm
      ref="formRef"
      :model="form"
      :rules="rules"
      label-width="120px"
      label-position="left"
      require-asterisk-position="right"
      class="product-create-form"
    >
      <ElFormItem :label="$t('product.fields.productId')" prop="product_id">
        <ElInput :model-value="form.product_id" readonly />
      </ElFormItem>
      <ElFormItem :label="$t('product.fields.name')" prop="name">
        <ElInput
          v-model="form.name"
          :placeholder="$t('product.createDialog.placeholders.name')"
        />
      </ElFormItem>
      <ElFormItem :label="$t('product.fields.timeoutSecs')" prop="timeout_secs">
        <ElInputNumber
          v-model="form.timeout_secs"
          :min="1"
          :max="300"
          controls-position="right"
          style="width: 100%"
        />
      </ElFormItem>
      <ElRow :gutter="20">
        <ElCol :span="12">
          <ElFormItem
            :label="$t('product.fields.enabled')"
            label-width="80px"
            class="inline-switch-item"
          >
            <ElSwitch v-model="form.enabled" />
          </ElFormItem>
        </ElCol>
        <ElCol :span="12">
          <ElFormItem
            :label="$t('product.fields.cbProEnabled')"
            prop="cb_pro_enabled"
            label-width="80px"
            class="inline-switch-item"
          >
            <ElSwitch v-model="form.cb_pro_enabled" />
          </ElFormItem>
        </ElCol>
      </ElRow>
    </ElForm>
  </EditDialog>
</template>

<script setup lang="ts">
import type { FormInstance, FormRules } from "element-plus";
import { $t } from "@/locales";
import {
  PRODUCT_ID_PATTERN,
  type CreateProductRequest,
  type ProductIdReservation,
} from "@/types/api/product";
import { createProduct } from "@/api/product";
import EditDialog from "@/components/common/EditDialog.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { logger } from "@/utils/sys";
defineOptions({ name: "CreateProductDialog" });
const props = defineProps<{
  reservation: ProductIdReservation | null;
}>();
const visible = defineModel<boolean>({ required: true });
const emit = defineEmits<{
  success: [];
}>();
const formRef = ref<FormInstance>();
const submitting = ref(false);
type CreateProductFormState = Omit<CreateProductRequest, "reservation_id"> & {
  product_id: string;
};
const createInitialFormState = (): CreateProductFormState => ({
  product_id: props.reservation?.product_id || "",
  name: "",
  enabled: true,
  timeout_secs: 30,
  cb_pro_enabled: false,
});
const form = reactive<CreateProductFormState>(createInitialFormState());
const rules: FormRules = {
  product_id: [
    {
      required: true,
      message: $t("product.createDialog.validation.productIdReservationFailed"),
      trigger: "change",
    },
    {
      pattern: PRODUCT_ID_PATTERN,
      message: $t("product.createDialog.validation.productIdPattern"),
      trigger: "change",
    },
  ],
  name: [
    {
      required: true,
      message: $t("product.createDialog.validation.nameRequired"),
      trigger: "blur",
    },
    {
      min: 2,
      max: 100,
      message: $t("product.createDialog.validation.nameLength"),
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
};
const handleSubmit = async () => {
  const reservation = props.reservation;
  if (!reservation?.id) {
    return;
  }
  const valid = await formRef.value?.validate().catch(() => false);
  if (!valid) return;
  await runWithLoading(submitting, async () => {
    const { product_id: _reservedProductId, ...payload } = toRaw(form);
    await createProduct({
      ...payload,
      reservation_id: reservation.id,
    });
    visible.value = false;
    emit("success");
    resetForm();
  }).catch((error) => {
    logger.error("Failed to create product:", error);
  });
};
const resetForm = () => {
  Object.assign(form, createInitialFormState());
  formRef.value?.clearValidate();
};
watch(visible, (val) => {
  if (val !== undefined) {
    resetForm();
  }
});
watch(
  () => props.reservation?.product_id,
  (nextProductId) => {
    if (visible.value) {
      form.product_id = nextProductId || "";
      if (nextProductId) {
        formRef.value?.clearValidate("product_id");
      } else {
        formRef.value?.validateField("product_id").catch(() => false);
      }
    }
  },
);
</script>

<style scoped lang="scss">
.product-create-form {
  padding: 10px 10px 0 10px;
}

.inline-switch-item {
  margin-bottom: 0;
  :deep(.el-form-item__content) {
    display: flex;
    align-items: center;
  }
}
</style>
