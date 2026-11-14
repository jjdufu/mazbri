<template>
  <EditDialog
    v-model="visible"
    :title="dialogTitle"
    width="520px"
    :confirm-text="$t('common.confirm')"
    :saving="saving"
    destroy-on-close
    @confirm="handleSubmit"
  >
    <ElForm
      ref="formRef"
      :model="form"
      :rules="rules"
      label-width="116px"
      @submit.prevent
    >
      <ElFormItem
        :label="$t('userManagement.detail.labels.displayName')"
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
        :label="$t('userManagement.detail.labels.email')"
        prop="email"
      >
        <ElInput
          v-model="form.email"
          :placeholder="$t('userManagement.createDialog.placeholders.email')"
        />
      </ElFormItem>
      <ElFormItem
        v-if="!isSuperAdminTarget"
        :label="$t('userManagement.detail.labels.role')"
        prop="role"
      >
        <ElSelect v-model="form.role" style="width: 100%">
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
      <ElFormItem
        v-if="!isSuperAdminTarget"
        :label="$t('userManagement.detail.labels.forcePasswordReset')"
      >
        <ElSwitch v-model="form.force_password_reset" />
      </ElFormItem>
      <ElFormItem
        v-if="!isSuperAdminTarget"
        :label="$t('userManagement.detail.labels.mfaRequired')"
      >
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
import { computed, reactive, ref, watch } from "vue";
import { ElMessage, type FormInstance, type FormRules } from "element-plus";
import { useI18n } from "vue-i18n";
import { $t } from "@/locales";
import type {
  AdminRole,
  AdminUser,
  UpdateAdminRequest,
} from "@/types/api/admin";
import { updateAdmin } from "@/api/admin";
import EditDialog from "@/components/common/EditDialog.vue";
import CommonInlineHint from "@/components/common/inline-hint/index.vue";
import MfaRequiredHint from "@/components/common/mfa-required-hint/index.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { logger } from "@/utils/sys";

defineOptions({ name: "UserEditDialog" });

const visible = defineModel<boolean>({ required: true });
const props = defineProps<{
  admin: AdminUser | null;
}>();
const emit = defineEmits<{
  success: [updated: AdminUser];
}>();

const { t } = useI18n();
const formRef = ref<FormInstance>();
const saving = ref(false);

const isSuperAdminTarget = computed(() => props.admin?.role === "R_ADMIN");
const dialogTitle = computed(() =>
  isSuperAdminTarget.value
    ? t("userManagement.detail.editTitle")
    : t("userManagement.detail.editUserTitle"),
);

const form = reactive<{
  display_name: string;
  email: string;
  role: AdminRole;
  mfa_required: boolean;
  force_password_reset: boolean;
}>({
  display_name: "",
  email: "",
  role: "R_VIEWER",
  mfa_required: false,
  force_password_reset: false,
});

const rules = computed<FormRules>(() => ({
  display_name: [
    {
      required: true,
      message: t("userManagement.detail.validation.displayNameRequired"),
      trigger: "blur",
    },
    {
      max: 100,
      message: t("userManagement.detail.validation.displayNameLength"),
      trigger: "blur",
    },
  ],
  email: [
    {
      type: "email",
      message: t("userManagement.detail.validation.emailInvalid"),
      trigger: "blur",
    },
  ],
  role: [
    {
      required: true,
      message: t("userManagement.detail.validation.roleRequired"),
      trigger: "change",
    },
  ],
}));

function syncFromAdmin(admin: AdminUser | null): void {
  if (!admin) return;
  form.display_name = admin.display_name ?? "";
  form.email = admin.email ?? "";
  form.role = admin.role;
  form.mfa_required = admin.mfa_required;
  form.force_password_reset = admin.force_password_reset;
}

async function handleSubmit(): Promise<void> {
  const admin = props.admin;
  if (!admin || saving.value) return;
  const valid = await formRef.value?.validate().catch(() => false);
  if (!valid) return;

  const payload: UpdateAdminRequest = {
    display_name: form.display_name.trim(),
    email: form.email.trim() || undefined,
  };
  if (!isSuperAdminTarget.value) {
    payload.role = form.role;
    payload.mfa_required = form.mfa_required;
    payload.force_password_reset = form.force_password_reset;
  }

  await runWithLoading(saving, async () => {
    const updated = await updateAdmin(admin.id, payload);
    visible.value = false;
    emit("success", updated);
    ElMessage.success($t("common.feedback.saveSuccess"));
  }).catch((error) => {
    logger.error("Failed to update admin:", error);
  });
}

watch(
  () => [visible.value, props.admin] as const,
  ([isOpen, admin]) => {
    if (isOpen && admin) {
      syncFromAdmin(admin);
    }
  },
  { immediate: true },
);
</script>
