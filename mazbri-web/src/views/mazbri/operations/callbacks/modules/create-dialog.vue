<template>
  <EditDialog
    v-model="visible"
    :title="$t('menus.mazbri.callbackFields.addTitle')"
    width="500px"
    :saving="loading"
    :confirm-text="$t('common.confirm')"
    :cancel-text="$t('common.cancel')"
    @confirm="handleSubmit"
    @close="handleClose"
  >
    <ElForm ref="formRef" :model="form" :rules="rules" label-width="120px">
      <ElFormItem :label="$t('callbacks.form.tenantCode')" prop="tenant_code">
        <ElInput
          v-model="form.tenant_code"
          :placeholder="$t('callbacks.placeholders.tenantCode')"
        />
      </ElFormItem>
      <ElFormItem :label="$t('callbacks.form.productCode')" prop="product_code">
        <ElInput
          v-model="form.product_code"
          :placeholder="$t('callbacks.placeholders.productCode')"
        />
      </ElFormItem>
      <ElFormItem
        :label="$t('menus.mazbri.callbackFields.url')"
        prop="callback_url"
      >
        <ElInput
          v-model="form.callback_url"
          :placeholder="$t('callbacks.placeholders.callbackUrl')"
        />
      </ElFormItem>
      <ElFormItem
        :label="$t('menus.mazbri.callbackFields.secret')"
        prop="secret_key"
      >
        <ElInput
          v-model="form.secret_key"
          :placeholder="$t('callbacks.placeholders.secretKey')"
        />
      </ElFormItem>
    </ElForm>
  </EditDialog>
</template>

<script setup lang="ts">
import { ref, watch } from "vue";
import type { FormInstance, FormRules } from "element-plus";
import { ElMessage } from "element-plus";
import { createCallbackSubscription } from "@/api/callback";
import { $t } from "@/locales";
import EditDialog from "@/components/common/EditDialog.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { getUserFacingErrorMessage } from "@/utils/http/error";
const props = defineProps<{
  modelValue: boolean;
}>();
const emits = defineEmits<{
  (e: "update:modelValue", value: boolean): void;
  (e: "success"): void;
}>();
const visible = ref(false);
const loading = ref(false);
const formRef = ref<FormInstance>();
const form = ref({
  tenant_code: "",
  product_code: "",
  callback_url: "",
  secret_key: "",
});
const rules: FormRules = {
  tenant_code: [
    {
      required: true,
      message: $t("callbacks.validation.tenantCodeRequired"),
      trigger: "blur",
    },
  ],
  product_code: [
    {
      required: true,
      message: $t("callbacks.validation.productCodeRequired"),
      trigger: "blur",
    },
  ],
  callback_url: [
    {
      required: true,
      message: $t("callbacks.validation.callbackUrlRequired"),
      trigger: "blur",
    },
  ],
  secret_key: [
    {
      required: true,
      message: $t("callbacks.validation.secretKeyRequired"),
      trigger: "blur",
    },
  ],
};
watch(
  () => props.modelValue,
  (val) => {
    visible.value = val;
  },
);
watch(visible, (val) => {
  emits("update:modelValue", val);
});
const handleClose = () => {
  formRef.value?.resetFields();
};
const handleSubmit = async () => {
  if (!formRef.value) return;
  const valid = await formRef.value.validate().catch(() => false);
  if (!valid) return;
  await runWithLoading(loading, async () => {
    try {
      await createCallbackSubscription(form.value);
      ElMessage.success($t("common.feedback.createSuccess"));
      emits("success");
      visible.value = false;
    } catch (err: unknown) {
      ElMessage.error(
        getUserFacingErrorMessage(err, $t("callbacks.messages.createFailed")),
      );
    }
  });
};
</script>
