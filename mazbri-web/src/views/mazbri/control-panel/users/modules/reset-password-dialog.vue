<template>
  <EditDialog
    v-model="visible"
    :title="$t('userManagement.resetPasswordDialog.title')"
    width="500px"
    :confirm-text="$t('common.confirm')"
    :saving="submitting"
    destroy-on-close
    @confirm="handleSubmit"
  >
    <ElForm
      ref="formRef"
      :model="form"
      :rules="rules"
      label-width="96px"
      @submit.prevent
    >
      <div v-if="admin" class="reset-target-banner">
        {{
          $t("userManagement.resetPasswordDialog.alert", {
            username: admin.username,
          })
        }}
      </div>
      <ElFormItem
        :label="$t('userManagement.resetPasswordDialog.fields.newPassword')"
        prop="new_password"
      >
        <div class="password-input-row">
          <ElInput
            v-model="form.new_password"
            type="password"
            :placeholder="
              $t('userManagement.resetPasswordDialog.placeholders.newPassword')
            "
            show-password
          />
          <ElTooltip
            :content="
              $t('userManagement.resetPasswordDialog.actions.regeneratePassword')
            "
            placement="top"
          >
            <ElButton
              :icon="RefreshRight"
              :aria-label="
                $t(
                  'userManagement.resetPasswordDialog.actions.regeneratePassword',
                )
              "
              @click="regeneratePassword"
            />
          </ElTooltip>
          <ElTooltip
            :content="
              $t('userManagement.resetPasswordDialog.actions.copyPassword')
            "
            placement="top"
          >
            <ElButton
              :icon="CopyDocument"
              :disabled="!form.new_password"
              :aria-label="
                $t('userManagement.resetPasswordDialog.actions.copyPassword')
              "
              @click="handleCopyPassword"
            />
          </ElTooltip>
        </div>
        <CommonInlineHint block class="mt-1.5">
          {{ $t("userManagement.resetPasswordDialog.tips.passwordGenerated") }}
        </CommonInlineHint>
      </ElFormItem>
      <ElFormItem>
        <ElCheckbox v-model="form.force_password_reset">
          {{ $t("userManagement.resetPasswordDialog.forceReset") }}
        </ElCheckbox>
      </ElFormItem>
      <MfaRequiredHint block />
    </ElForm>
  </EditDialog>
</template>

<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { ElMessage, type FormInstance, type FormRules } from "element-plus";
import { CopyDocument, RefreshRight } from "@element-plus/icons-vue";
import { useI18n } from "vue-i18n";
import { $t } from "@/locales";
import type { AdminUser, ResetPasswordRequest } from "@/types/api/admin";
import type { PasswordPolicy } from "@/types/api/settings";
import { resetAdminPassword } from "@/api/admin";
import { fetchPasswordPolicy } from "@/api/audit";
import EditDialog from "@/components/common/EditDialog.vue";
import CommonInlineHint from "@/components/common/inline-hint/index.vue";
import MfaRequiredHint from "@/components/common/mfa-required-hint/index.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { logger } from "@/utils/sys";
import {
  generatePasswordFromPolicy,
  generatePasswordWithFetch,
  resolvePasswordPolicyPayload,
  validatePasswordAgainstPolicy,
} from "@/utils/password";
import { copyWithFeedback } from "@/utils/ui/copy-feedback";

defineOptions({ name: "ResetPasswordDialog" });

const visible = defineModel<boolean>({ required: true });
const props = defineProps<{
  admin: AdminUser | null;
}>();
const emit = defineEmits<{
  success: [];
}>();

const { t } = useI18n();
const formRef = ref<FormInstance>();
const submitting = ref(false);
const passwordPolicy = ref<PasswordPolicy | null>(null);

const form = reactive<Required<ResetPasswordRequest>>({
  new_password: "",
  force_password_reset: true,
});

const rules = computed<FormRules>(() => ({
  new_password: [
    {
      required: true,
      message: t(
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
        const message = validatePasswordAgainstPolicy(
          value,
          passwordPolicy.value,
          (key, params) => t(key, params ?? {}),
        );
        if (message) {
          callback(new Error(message));
          return;
        }
        callback();
      },
      trigger: ["blur", "change"],
    },
  ],
}));

async function loadPolicyAndGenerate(): Promise<void> {
  try {
    const response = await fetchPasswordPolicy();
    const policy = resolvePasswordPolicyPayload(response);
    passwordPolicy.value = policy;
    form.new_password = policy
      ? generatePasswordFromPolicy(policy)
      : await generatePasswordWithFetch();
  } catch {
    form.new_password = await generatePasswordWithFetch();
  }
}

async function regeneratePassword(): Promise<void> {
  form.new_password = passwordPolicy.value
    ? generatePasswordFromPolicy(passwordPolicy.value)
    : await generatePasswordWithFetch();
  formRef.value?.clearValidate("new_password");
}

function handleCopyPassword(): void {
  if (!form.new_password) return;
  void copyWithFeedback(form.new_password);
}

async function handleSubmit(): Promise<void> {
  const admin = props.admin;
  if (!admin || submitting.value) return;
  const valid = await formRef.value?.validate().catch(() => false);
  if (!valid) return;
  await runWithLoading(submitting, async () => {
    await resetAdminPassword(admin.id, {
      new_password: form.new_password,
      force_password_reset: form.force_password_reset,
    });
    visible.value = false;
    emit("success");
    resetForm();
    ElMessage.success($t("common.feedback.resetPasswordSuccess"));
  }).catch((error) => {
    logger.error("Failed to reset password:", error);
  });
}

function resetForm(): void {
  form.new_password = "";
  form.force_password_reset = true;
  formRef.value?.resetFields();
}

watch(
  visible,
  (val) => {
    if (val) {
      void loadPolicyAndGenerate();
    } else {
      resetForm();
    }
  },
  { immediate: true },
);
</script>

<style scoped lang="scss">
.reset-target-banner {
  margin-bottom: 16px;
  padding: 8px 12px;
  border-radius: var(--el-border-radius-base);
  background-color: var(--el-fill-color-lighter);
  color: var(--el-text-color-regular);
  font-size: 13px;
  line-height: 1.5;
}

.password-input-row {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;

  :deep(.el-input) {
    flex: 1;
    min-width: 0;
  }

  :deep(.el-button + .el-button) {
    margin-left: 0;
  }
}
</style>
