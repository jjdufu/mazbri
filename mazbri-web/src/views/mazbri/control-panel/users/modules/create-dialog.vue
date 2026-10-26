<template>
  <EditDialog
    v-model="visible"
    :title="$t('userManagement.createDialog.title')"
    width="500px"
    :confirm-text="$t('common.confirm')"
    :saving="submitting"
    @confirm="handleSubmit"
  >
    <ElForm ref="formRef" :model="form" :rules="rules" label-width="100px">
      <ElFormItem
        :label="$t('userManagement.createDialog.fields.username')"
        prop="username"
      >
        <ElInput
          v-model="form.username"
          :placeholder="$t('userManagement.createDialog.placeholders.username')"
        />
      </ElFormItem>
      <ElFormItem
        :label="$t('userManagement.createDialog.fields.displayName')"
        prop="display_name"
      >
        <ElInput
          v-model="form.display_name"
          :placeholder="
            $t('userManagement.createDialog.placeholders.displayName')
          "
        />
      </ElFormItem>
      <ElFormItem
        :label="$t('userManagement.createDialog.fields.email')"
        prop="email"
      >
        <ElInput
          v-model="form.email"
          :placeholder="$t('userManagement.createDialog.placeholders.email')"
        />
      </ElFormItem>
      <ElFormItem
        :label="$t('userManagement.createDialog.fields.password')"
        prop="password"
      >
        <ElInput
          v-model="form.password"
          type="password"
          :placeholder="$t('userManagement.createDialog.placeholders.password')"
          show-password
        >
          <template #suffix>
            <ElIcon
              class="el-input__icon c-p hover:text-[var(--el-color-primary)] transition-colors ml-1.5"
              :title="
                $t('userManagement.createDialog.actions.regeneratePassword')
              "
              @click.stop="regeneratePassword"
            >
              <RefreshRight />
            </ElIcon>
          </template>
        </ElInput>
        <CommonInlineHint block class="mt-1.5">
          {{ $t("userManagement.createDialog.tips.passwordGenerated") }}
        </CommonInlineHint>
      </ElFormItem>
      <ElFormItem
        :label="$t('userManagement.createDialog.fields.role')"
        prop="role"
      >
        <ElSelect
          v-model="form.role"
          :placeholder="$t('userManagement.createDialog.placeholders.role')"
          style="width: 100%"
        >
          <ElOption
            :label="$t('userManagement.roles.financeWithScope')"
            value="R_FINANCE"
          />
          <ElOption
            :label="$t('userManagement.roles.viewerWithScope')"
            value="R_VIEWER"
          />
        </ElSelect>
      </ElFormItem>
      <ElFormItem :label="$t('userManagement.createDialog.fields.firstLogin')">
        <ElSwitch v-model="form.force_password_reset" />
        <CommonInlineHint class="ml-2">
          {{ $t("userManagement.createDialog.tips.forcePasswordReset") }}
        </CommonInlineHint>
      </ElFormItem>
    </ElForm>
  </EditDialog>
</template>

<script setup lang="ts">
import type { FormInstance, FormRules } from "element-plus";
import { RefreshRight } from "@element-plus/icons-vue";
import { $t } from "@/locales";
import type { CreateAdminRequest } from "@/types/api/admin";
import { createAdmin } from "@/api/admin";
import EditDialog from "@/components/common/EditDialog.vue";
import CommonInlineHint from "@/components/common/inline-hint/index.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { logger } from "@/utils/sys";
import { generatePasswordWithFetch } from "@/utils/password";
defineOptions({ name: "CreateUserDialog" });
const visible = defineModel<boolean>({ required: true });
const emit = defineEmits<{
  success: [];
}>();
const formRef = ref<FormInstance>();
const submitting = ref(false);
const form = reactive<CreateAdminRequest>({
  username: "",
  display_name: "",
  email: "",
  password: "",
  role: "R_VIEWER",
  force_password_reset: true,
});
const rules: FormRules = {
  username: [
    {
      required: true,
      message: $t("userManagement.createDialog.validation.usernameRequired"),
      trigger: "blur",
    },
    {
      min: 3,
      max: 50,
      message: $t("userManagement.createDialog.validation.usernameLength"),
      trigger: "blur",
    },
    {
      pattern: /^[a-zA-Z][a-zA-Z0-9_-]*$/,
      message: $t("userManagement.createDialog.validation.usernamePattern"),
      trigger: "blur",
    },
  ],
  display_name: [
    {
      required: true,
      message: $t("userManagement.createDialog.validation.displayNameRequired"),
      trigger: "blur",
    },
    {
      max: 100,
      message: $t("userManagement.createDialog.validation.displayNameLength"),
      trigger: "blur",
    },
  ],
  email: [
    {
      required: true,
      message: $t("userManagement.createDialog.validation.emailRequired"),
      trigger: "blur",
    },
    {
      type: "email",
      message: $t("userManagement.createDialog.validation.emailInvalid"),
      trigger: "blur",
    },
  ],
  password: [
    {
      required: true,
      message: $t("userManagement.createDialog.validation.passwordRequired"),
      trigger: "blur",
    },
    {
      min: 8,
      message: $t("userManagement.createDialog.validation.passwordLength"),
      trigger: "blur",
    },
  ],
  role: [
    {
      required: true,
      message: $t("userManagement.createDialog.validation.roleRequired"),
      trigger: "change",
    },
  ],
};
const regeneratePassword = async () => {
  form.password = await generatePasswordWithFetch();
};
const handleSubmit = async () => {
  if (submitting.value) return;
  const valid = await formRef.value?.validate().catch(() => false);
  if (!valid) return;
  await runWithLoading(submitting, async () => {
    await createAdmin(toRaw(form));
    visible.value = false;
    emit("success");
    resetForm();
    ElMessage.success($t("userManagement.createDialog.messages.createSuccess"));
  }).catch((error) => {
    logger.error("Failed to create user:", error);
  });
};
const resetForm = () => {
  form.username = "";
  form.display_name = "";
  form.email = "";
  form.password = "";
  form.role = "R_VIEWER";
  form.force_password_reset = true;
  formRef.value?.resetFields();
};
watch(visible, async (val) => {
  if (val) {
    form.password = await generatePasswordWithFetch();
  } else {
    resetForm();
  }
});
</script>
