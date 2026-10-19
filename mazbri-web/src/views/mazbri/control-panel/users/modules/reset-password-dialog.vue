<template>
  <EditDialog
    v-model="visible"
    :title="$t('userManagement.resetPasswordDialog.title')"
    width="450px"
    :confirm-text="$t('common.confirm')"
    :saving="submitting"
    @confirm="handleSubmit"
  >
    <ElForm ref="formRef" :model="form" :rules="rules" label-width="100px" @submit.prevent>
      <ElFormItem
        :label="$t('userManagement.resetPasswordDialog.fields.newPassword')"
        prop="new_password"
      >
        <ElInput
          v-model="form.new_password"
          type="password"
          :placeholder="
            $t('userManagement.resetPasswordDialog.placeholders.newPassword')
          "
          show-password
        />
      </ElFormItem>
      <ElFormItem>
        <ElCheckbox v-model="forceReset">
          {{ $t("userManagement.resetPasswordDialog.forceReset") }}
        </ElCheckbox>
      </ElFormItem>
    </ElForm>
  </EditDialog>
</template>

<script setup lang="ts">
import { ElMessage, type FormInstance, type FormRules } from "element-plus";
import { $t } from "@/locales";
import type { AdminUser, ResetPasswordRequest } from "@/types/api/admin";
import { resetAdminPassword, forcePasswordReset } from "@/api/admin";
import EditDialog from "@/components/common/EditDialog.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { logger } from "@/utils/sys";
defineOptions({ name: "ResetPasswordDialog" });
const visible = defineModel<boolean>({ required: true });
const props = defineProps<{
  admin: AdminUser | null;
}>();
const emit = defineEmits<{
  success: [];
}>();
const formRef = ref<FormInstance>();
const submitting = ref(false);
const forceReset = ref(true);
const form = reactive<ResetPasswordRequest>({
  new_password: "",
});
const rules: FormRules = {
  new_password: [
    {
      required: true,
      message: $t(
        "userManagement.resetPasswordDialog.validation.newPasswordRequired",
      ),
      trigger: "blur",
    },
    {
      validator: (
        _rule: unknown,
        value: string,
        callback: (error?: Error) => void,
      ) => {
        if (!value) {
          callback();
          return;
        }
        if (value.length < 8) {
          callback(
            new Error(
              $t(
                "userManagement.resetPasswordDialog.validation.newPasswordLength",
              ),
            ),
          );
          return;
        }
        const hasUpper = /[A-Z]/.test(value);
        const hasLower = /[a-z]/.test(value);
        const hasDigit = /[0-9]/.test(value);
        if (!hasUpper || !hasLower || !hasDigit) {
          callback(new Error("密码必须包含大小写字母和数字"));
          return;
        }
        callback();
      },
      trigger: ["blur", "change"],
    },
  ],
};
const handleSubmit = async () => {
  const admin = props.admin;
  if (!admin) return;
  const valid = await formRef.value?.validate().catch(() => false);
  if (!valid) return;
  await runWithLoading(submitting, async () => {
    await resetAdminPassword(admin.id, toRaw(form));
    if (forceReset.value) {
      await forcePasswordReset(admin.id, { force_password_reset: true });
    }
    visible.value = false;
    emit("success");
    resetForm();
    ElMessage.success($t("common.feedback.resetPasswordSuccess"));
  }).catch((error) => {
    logger.error("Failed to reset password:", error);
  });
};
const resetForm = () => {
  form.new_password = "";
  forceReset.value = true;
  formRef.value?.resetFields();
};
watch(visible, (val) => {
  if (!val) {
    resetForm();
  }
});
</script>
