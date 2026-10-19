<template>
  <EditDialog
    v-model="visible"
    :title="$t('tenant.actions.addTenant')"
    width="550px"
    :confirm-text="$t('common.confirm')"
    :saving="submitting"
    @confirm="handleSubmit"
  >
    <ElForm ref="formRef" :model="form" :rules="rules" label-width="100px">
      <ElFormItem :label="$t('tenant.fields.code')" prop="tenant_code">
        <ElInput
          v-model="form.tenant_code"
          :placeholder="$t('tenant.createDialog.placeholders.code')"
          maxlength="8"
          @input="handleTenantCodeInput"
        />
        <div class="form-tip">{{ $t("tenant.createDialog.tips.code") }}</div>
      </ElFormItem>
      <ElFormItem :label="$t('tenant.fields.accountId')" prop="tenant_id">
        <ElInput
          v-model="form.tenant_id"
          :placeholder="$t('tenant.createDialog.placeholders.accountId')"
          @input="handleTenantIdInput"
        />
        <div class="form-tip">
          {{ $t("tenant.createDialog.tips.accountId") }}
        </div>
      </ElFormItem>
      <ElFormItem :label="$t('common.fields.name')" prop="name">
        <ElInput
          v-model="form.name"
          :placeholder="$t('tenant.createDialog.placeholders.name')"
        />
      </ElFormItem>
      <ElFormItem
        :label="$t('tenant.fields.contactPerson')"
        prop="contact_person"
      >
        <ElInput
          v-model="form.contact_person"
          :placeholder="$t('tenant.createDialog.placeholders.contactPerson')"
        />
      </ElFormItem>
      <ElFormItem
        :label="$t('tenant.fields.contactPhone')"
        prop="contact_phone"
      >
        <ElInput
          v-model="form.contact_phone"
          :placeholder="$t('tenant.createDialog.placeholders.contactPhone')"
        />
      </ElFormItem>
      <ElFormItem
        :label="$t('tenant.fields.contactEmail')"
        prop="contact_email"
      >
        <ElInput
          v-model="form.contact_email"
          :placeholder="$t('tenant.createDialog.placeholders.contactEmail')"
        />
      </ElFormItem>
    </ElForm>
  </EditDialog>
</template>

<script setup lang="ts">
import type { FormInstance, FormRules } from "element-plus";
import { $t } from "@/locales";
import type { CreateTenantRequest } from "@/types/api/tenant";
import { createTenant } from "@/api/tenant";
import EditDialog from "@/components/common/EditDialog.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { logger } from "@/utils/sys";
defineOptions({ name: "CreateTenantDialog" });
const visible = defineModel<boolean>({ required: true });
const emit = defineEmits<{
  success: [];
}>();
const formRef = ref<FormInstance>();
const submitting = ref(false);
const generateTenantCode = () => {
  const chars = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
  let code = "";
  for (let i = 0; i < 8; i++) {
    code += chars.charAt(Math.floor(Math.random() * chars.length));
  }
  return code;
};
const form = reactive<CreateTenantRequest>({
  tenant_code: "",
  tenant_id: "",
  name: "",
  contact_person: "",
  contact_phone: "",
  contact_email: "",
});
const rules: FormRules = {
  tenant_code: [
    {
      required: true,
      message: $t("tenant.createDialog.validation.codeRequired"),
      trigger: "blur",
    },
    {
      len: 8,
      message: $t("tenant.createDialog.validation.codeLength"),
      trigger: "blur",
    },
    {
      pattern: /^[A-Z0-9]{8}$/,
      message: $t("tenant.createDialog.validation.codePattern"),
      trigger: "blur",
    },
  ],
  tenant_id: [
    {
      required: true,
      message: $t("tenant.createDialog.validation.accountIdRequired"),
      trigger: "blur",
    },
    {
      pattern: /^[a-z0-9_-]+$/,
      message: $t("tenant.createDialog.validation.accountIdPattern"),
      trigger: "blur",
    },
  ],
  name: [
    {
      required: true,
      message: $t("tenant.createDialog.validation.nameRequired"),
      trigger: "blur",
    },
    {
      min: 2,
      max: 100,
      message: $t("tenant.createDialog.validation.nameLength"),
      trigger: "blur",
    },
  ],
  contact_person: [
    {
      required: true,
      message: $t("tenant.createDialog.validation.contactPersonRequired"),
      trigger: "blur",
    },
    {
      min: 1,
      max: 50,
      message: $t("tenant.createDialog.validation.contactPersonLength"),
      trigger: "blur",
    },
  ],
  contact_phone: [
    {
      required: true,
      message: $t("tenant.createDialog.validation.contactPhoneRequired"),
      trigger: "blur",
    },
    {
      pattern: /^1[3-9]\d{9}$/,
      message: $t("tenant.createDialog.validation.contactPhonePattern"),
      trigger: "blur",
    },
  ],
  contact_email: [
    {
      required: true,
      message: $t("tenant.createDialog.validation.contactEmailRequired"),
      trigger: "blur",
    },
    {
      type: "email",
      message: $t("tenant.createDialog.validation.contactEmailPattern"),
      trigger: "blur",
    },
  ],
};
const handleTenantCodeInput = (value: string) => {
  form.tenant_code = value.toUpperCase();
};
const handleTenantIdInput = (value: string) => {
  form.tenant_id = value.toLowerCase();
};
const handleSubmit = async () => {
  const valid = await formRef.value?.validate().catch(() => false);
  if (!valid) return;
  await runWithLoading(submitting, async () => {
    await createTenant(toRaw(form));
    visible.value = false;
    emit("success");
    resetForm();
  }).catch((error) => {
    logger.error("Failed to create tenant:", error);
  });
};
const resetForm = () => {
  form.tenant_code = generateTenantCode();
  form.tenant_id = "";
  form.name = "";
  form.contact_person = "";
  form.contact_phone = "";
  form.contact_email = "";
  formRef.value?.resetFields();
};
watch(visible, (val) => {
  if (val) {
    form.tenant_code = generateTenantCode();
  }
});
watch(visible, (val) => {
  if (!val) {
    resetForm();
  }
});
</script>

<style scoped>
.form-tip {
  font-size: 12px;
  color: #909399;
  margin-top: 4px;
}
</style>
