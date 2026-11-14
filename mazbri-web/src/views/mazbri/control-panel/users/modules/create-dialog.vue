<template>
  <EditDialog
    v-model="visible"
    :title="$t('userManagement.createDialog.title')"
    width="520px"
    :confirm-text="$t('common.confirm')"
    :saving="submitting"
    destroy-on-close
    @confirm="handleSubmit"
  >
    <ElForm
      ref="formRef"
      :model="form"
      :rules="rules"
      label-width="104px"
      @submit.prevent
    >
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
        <div class="password-input-row">
          <ElInput
            v-model="form.password"
            type="password"
            :placeholder="
              $t('userManagement.createDialog.placeholders.password')
            "
            show-password
          />
          <ElTooltip
            :content="$t('userManagement.createDialog.actions.regeneratePassword')"
            placement="top"
          >
            <ElButton
              :icon="RefreshRight"
              :aria-label="
                $t('userManagement.createDialog.actions.regeneratePassword')
              "
              @click="regeneratePassword"
            />
          </ElTooltip>
          <ElTooltip
            :content="$t('userManagement.createDialog.actions.copyPassword')"
            placement="top"
          >
            <ElButton
              :icon="CopyDocument"
              :disabled="!form.password"
              :aria-label="$t('userManagement.createDialog.actions.copyPassword')"
              @click="handleCopyPassword"
            />
          </ElTooltip>
        </div>
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
      <ElFormItem :label="$t('userManagement.createDialog.fields.mfaRequired')">
        <ElSwitch v-model="form.mfa_required" />
        <CommonInlineHint class="ml-2">
          {{ $t("userManagement.createDialog.tips.mfaRequired") }}
        </CommonInlineHint>
      </ElFormItem>
      <MfaRequiredHint block />
    </ElForm>
  </EditDialog>
</template>

<script setup lang="ts">
import { computed, reactive, ref, toRaw, watch } from "vue";
import { ElMessage, type FormInstance, type FormRules } from "element-plus";
import { CopyDocument, RefreshRight } from "@element-plus/icons-vue";
import { useI18n } from "vue-i18n";
import { $t } from "@/locales";
import type { CreateAdminRequest } from "@/types/api/admin";
import type { PasswordPolicy } from "@/types/api/settings";
import { createAdmin } from "@/api/admin";
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

defineOptions({ name: "CreateUserDialog" });

const visible = defineModel<boolean>({ required: true });
const emit = defineEmits<{
  success: [];
}>();

const { t } = useI18n();
const formRef = ref<FormInstance>();
const submitting = ref(false);
const passwordPolicy = ref<PasswordPolicy | null>(null);

const form = reactive<CreateAdminRequest>({
  username: "",
  display_name: "",
  email: "",
  password: "",
  role: "R_VIEWER",
  mfa_required: false,
  force_password_reset: true,
});

const rules = computed<FormRules>(() => ({
  username: [
    {
      required: true,
      message: t("userManagement.createDialog.validation.usernameRequired"),
      trigger: "blur",
    },
    {
      min: 3,
      max: 50,
      message: t("userManagement.createDialog.validation.usernameLength"),
      trigger: "blur",
    },
    {
      pattern: /^[a-zA-Z][a-zA-Z0-9_-]*$/,
      message: t("userManagement.createDialog.validation.usernamePattern"),
      trigger: "blur",
    },
  ],
  display_name: [
    {
      required: true,
      message: t("userManagement.createDialog.validation.displayNameRequired"),
      trigger: "blur",
    },
    {
      max: 100,
      message: t("userManagement.createDialog.validation.displayNameLength"),
      trigger: "blur",
    },
  ],
  email: [
    {
      required: true,
      message: t("userManagement.createDialog.validation.emailRequired"),
      trigger: "blur",
    },
    {
      type: "email",
      message: t("userManagement.createDialog.validation.emailInvalid"),
      trigger: "blur",
    },
  ],
  password: [
    {
      required: true,
      message: t("userManagement.createDialog.validation.passwordRequired"),
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
  role: [
    {
      required: true,
      message: t("userManagement.createDialog.validation.roleRequired"),
      trigger: "change",
    },
  ],
}));

async function loadPolicyAndGenerate(): Promise<void> {
  try {
    const response = await fetchPasswordPolicy();
    const policy = resolvePasswordPolicyPayload(response);
    passwordPolicy.value = policy;
    form.password = policy
      ? generatePasswordFromPolicy(policy)
      : await generatePasswordWithFetch();
  } catch {
    form.password = await generatePasswordWithFetch();
  }
}

async function regeneratePassword(): Promise<void> {
  form.password = passwordPolicy.value
    ? generatePasswordFromPolicy(passwordPolicy.value)
    : await generatePasswordWithFetch();
  formRef.value?.clearValidate("password");
}

function handleCopyPassword(): void {
  if (!form.password) return;
  void copyWithFeedback(form.password);
}

async function handleSubmit(): Promise<void> {
  if (submitting.value) return;
  const valid = await formRef.value?.validate().catch(() => false);
  if (!valid) return;
  await runWithLoading(submitting, async () => {
    await createAdmin(toRaw(form), { showSuccessMessage: false });
    visible.value = false;
    emit("success");
    resetForm();
    ElMessage.success($t("userManagement.createDialog.messages.createSuccess"));
  }).catch((error) => {
    logger.error("Failed to create user:", error);
  });
}

function resetForm(): void {
  form.username = "";
  form.display_name = "";
  form.email = "";
  form.password = "";
  form.role = "R_VIEWER";
  form.mfa_required = false;
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
