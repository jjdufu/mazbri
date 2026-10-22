<template>
  <EditDialog
    v-model="visible"
    :title="drawerTitle"
    width="560px"
    :saving="saving"
    :confirm-disabled="!currentProduct || loading"
    @confirm="handleSave"
    @close="handleClose"
  >
    <div v-loading="loading" class="dialog-body">
      <ElForm label-width="120px">
        <ElDivider content-position="left">{{
          $t("authorization.config.billingSection")
        }}</ElDivider>
        <ElFormItem :label="$t('authorization.config.unitPrice')">
          <ElInput
            v-model="form.unit_price"
            :placeholder="$t('authorization.config.unitPricePlaceholder')"
          />
        </ElFormItem>

        <ElDivider content-position="left">{{
          $t("authorization.config.fallbackSection")
        }}</ElDivider>
        <ElFormItem :label="$t('authorization.config.cbProFallback')">
          <ElSwitch
            v-model="form.cb_pro_fallback"
            :disabled="!currentProduct?.cb_pro_enabled"
          />
        </ElFormItem>

        <ElDivider content-position="left">{{
          $t("authorization.config.codecSection")
        }}</ElDivider>
        <ElFormItem :label="$t('authorization.config.codecEnabled')">
          <ElSwitch v-model="form.codec_enabled" />
        </ElFormItem>
        <ElFormItem :label="$t('authorization.config.requestCodec')">
          <ElSelect
            v-model="form.request_codec"
            style="width: 100%"
            :disabled="!form.codec_enabled"
          >
            <ElOption label="JSON" value="json" />
            <ElOption label="Fury" value="fury" />
          </ElSelect>
        </ElFormItem>
        <ElFormItem :label="$t('authorization.config.responseCodec')">
          <ElSelect
            v-model="form.response_codec"
            style="width: 100%"
            :disabled="!form.codec_enabled"
          >
            <ElOption label="JSON" value="json" />
            <ElOption label="Fury" value="fury" />
          </ElSelect>
        </ElFormItem>
        <ElFormItem :label="$t('authorization.config.codecVersion')">
          <ElInputNumber
            v-model="form.codec_version"
            :min="1"
            :max="99"
            :step="1"
            :disabled="!form.codec_enabled"
          />
        </ElFormItem>
      </ElForm>
    </div>
  </EditDialog>
</template>

<script setup lang="ts">
import { ElMessage, ElMessageBox } from "element-plus";
import { $t } from "@/locales";
import EditDialog from "@/components/common/EditDialog.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import {
  fetchTenantProductConfig,
  updateTenantProductConfig,
  mergeTenantProductConfig,
  createDefaultCBProPolicyConfig,
} from "@/api/tenant";
import type {
  PayloadCodec,
  TenantProduct,
  TenantProductConfig,
} from "@/types/api/tenant";
import { HttpError } from "@/utils/http/error";
import { logger } from "@/utils/sys";
import { summarizeTenantProductConflict } from "./conflict-summary";
defineOptions({ name: "ChannelConfigDialog" });
const props = defineProps<{
  modelValue: boolean;
  tenantCode: string;
  product: TenantProduct | null;
}>();
const emit = defineEmits<{
  "update:modelValue": [value: boolean];
  updated: [];
}>();
const loading = ref(false);
const saving = ref(false);
const currentProduct = ref<TenantProduct | null>(null);
const visible = computed({
  get: () => props.modelValue,
  set: (value: boolean) => emit("update:modelValue", value),
});
const form = reactive<TenantProductConfig>({
  unit_price: "0.001",
  currency: "CNY",
  cb_pro_fallback: false,
  cbpro: createDefaultCBProPolicyConfig(),
  request_codec: "json" as PayloadCodec,
  response_codec: "json" as PayloadCodec,
  codec_enabled: false,
  codec_version: 1,
});
const activeProduct = computed(() => currentProduct.value ?? props.product);
const drawerTitle = computed(() =>
  activeProduct.value
    ? `${$t("authorization.config.title")} · ${activeProduct.value.name}`
    : $t("authorization.config.title"),
);
const syncFromProduct = (product: TenantProduct | null) => {
  if (!product) return;
  currentProduct.value = {
    ...product,
    config: mergeTenantProductConfig(product.config),
  };
  form.unit_price = product.config.unit_price;
  form.currency = "CNY";
  form.cb_pro_fallback = product.cb_pro_enabled
    ? product.config.cb_pro_fallback
    : false;
  form.request_codec = product.config.request_codec;
  form.response_codec = product.config.response_codec;
  form.codec_enabled = product.config.codec_enabled;
  form.codec_version = product.config.codec_version;
  form.cbpro = mergeTenantProductConfig(product.config).cbpro;
};
const handleClose = () => {
  emit("update:modelValue", false);
};
const loadCurrentConfig = async () => {
  const product = props.product;
  if (!product) return;
  syncFromProduct(product);
  await runWithLoading(loading, async () => {
    const latestProduct = await fetchTenantProductConfig(
      props.tenantCode,
      product.product_id,
    );
    syncFromProduct(latestProduct);
  }).catch((error) => {
    logger.error("Failed to load latest tenant product config:", error);
  });
};
const handleSave = async () => {
  const product = currentProduct.value ?? props.product;
  if (!product) return;
  const submittedPayload: TenantProductConfig = {
    ...form,
    currency: "CNY",
    cb_pro_fallback: product.cb_pro_enabled ? form.cb_pro_fallback : false,
    codec_enabled: form.codec_enabled,
    request_codec: form.request_codec,
    response_codec: form.response_codec,
    codec_version: form.codec_version,
    cbpro: mergeTenantProductConfig(form).cbpro,
  };
  await runWithLoading(saving, async () => {
    try {
      await updateTenantProductConfig(props.tenantCode, product.product_id, {
        ...submittedPayload,
        expected_updated_at: product.updated_at,
      });
      ElMessage.success($t("authorization.config.saveSuccess"));
      emit("updated");
      handleClose();
    } catch (error) {
      if (error instanceof HttpError && error.code === 409) {
        await loadCurrentConfig();
        const latest = currentProduct.value ?? props.product;
        if (latest) {
          await ElMessageBox.alert(
            summarizeTenantProductConflict(
              submittedPayload,
              latest.config,
              latest.updated_at,
            ),
            $t("authorization.config.conflictTitle"),
            { type: "warning" },
          );
        }
      }
      logger.error("Failed to update tenant product config:", error);
    }
  });
};
watch(
  () => [props.modelValue, props.product] as const,
  async ([visible]) => {
    if (visible) {
      await loadCurrentConfig();
    }
  },
  { immediate: true },
);
</script>

<style scoped lang="scss">
.dialog-body {
  padding-top: 4px;
}
</style>
