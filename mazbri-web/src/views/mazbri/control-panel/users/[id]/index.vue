<template>
  <div class="admin-detail-page">
    <ElPageHeader @back="router.back()">
      <template #content>
        <span class="text-lg font-medium">{{
          admin?.role === 'R_ADMIN' ? $t("userManagement.detail.title") : $t("userManagement.detail.userTitle")
        }}</span>
      </template>
    </ElPageHeader>

    <ElCard
      :loading="loading"
      shadow="never"
      class="mt-5 surface-settings-card"
    >
      <template #header>
        <SurfaceCardHeader :title="$t('userManagement.detail.basicInfo')">
          <template #actions>
            <ElButton type="primary" @click="handleEdit">{{
              $t("common.actions.modify")
            }}</ElButton>
          </template>
        </SurfaceCardHeader>
      </template>

      <ElDescriptions :column="2" border>
        <ElDescriptionsItem :label="$t('userManagement.detail.labels.id')">{{
          admin?.id
        }}</ElDescriptionsItem>
        <ElDescriptionsItem
          :label="$t('userManagement.detail.labels.username')"
          >{{ admin?.username }}</ElDescriptionsItem
        >
        <ElDescriptionsItem :label="$t('userManagement.detail.labels.email')">{{
          admin?.email
        }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('userManagement.detail.labels.role')">
          <ElTag :type="getRoleType(admin?.role || 'R_VIEWER')">
            {{ getRoleLabel(admin?.role || "R_VIEWER") }}
          </ElTag>
        </ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('userManagement.detail.labels.status')">
          <ElSwitch
            :model-value="admin?.is_active"
            :loading="statusLoading"
            :active-text="$t('common.status.active')"
            :inactive-text="$t('common.status.disabled')"
            @change="handleStatusChange"
          />
        </ElDescriptionsItem>
        <ElDescriptionsItem
          :label="$t('userManagement.detail.labels.mfaStatus')"
        >
          <ElTag :type="admin?.mfa_enabled ? 'success' : 'info'">
            {{
              admin?.mfa_enabled
                ? $t("common.status.enabled")
                : $t("common.status.disabled")
            }}
          </ElTag>
        </ElDescriptionsItem>
        <ElDescriptionsItem
          :label="$t('userManagement.detail.labels.forcePasswordReset')"
        >
          <ElTag :type="admin?.force_password_reset ? 'warning' : 'success'">
            {{
              admin?.force_password_reset
                ? $t("common.status.enabled")
                : $t("common.status.disabled")
            }}
          </ElTag>
        </ElDescriptionsItem>
        <ElDescriptionsItem
          :label="$t('userManagement.detail.labels.lastLogin')"
        >
          {{ admin?.last_login_at ? formatDate(admin.last_login_at) : "-" }}
        </ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('common.fields.createdAt')">{{
          formatDate(admin?.created_at || "")
        }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('common.fields.updatedAt')">{{
          formatDate(admin?.updated_at || "")
        }}</ElDescriptionsItem>
      </ElDescriptions>

      <EditDialog
        v-model="editVisible"
        :title="admin?.role === 'R_ADMIN' ? $t('userManagement.detail.editTitle') : $t('userManagement.detail.editUserTitle')"
        width="560px"
        :saving="saving"
        @confirm="handleSave"
      >
        <ElForm
          ref="formRef"
          :model="editForm"
          :rules="rules"
          label-width="100px"
        >
          <ElFormItem
            :label="$t('userManagement.detail.labels.email')"
            prop="email"
          >
            <ElInput v-model="editForm.email" />
          </ElFormItem>
          <ElFormItem
            v-if="admin?.role !== 'R_ADMIN'"
            :label="$t('userManagement.detail.labels.role')"
            prop="role"
          >
            <ElSelect v-model="editForm.role" style="width: 100%">
              <ElOption
                :label="$t('userManagement.roles.operator')"
                value="R_OPERATOR"
              />
              <ElOption
                :label="$t('userManagement.roles.finance')"
                value="R_FINANCE"
              />
              <ElOption
                :label="$t('userManagement.roles.viewer')"
                value="R_VIEWER"
              />
            </ElSelect>
          </ElFormItem>
          <ElFormItem
            :label="$t('userManagement.detail.labels.forcePasswordReset')"
          >
            <ElSwitch v-model="editForm.force_password_reset" />
          </ElFormItem>
        </ElForm>
      </EditDialog>
    </ElCard>

    <ElCard shadow="never" class="mt-5 surface-settings-card">
      <template #header>
        <SurfaceCardHeader :title="$t('common.fields.operations')" />
      </template>
      <CommonInlineRow gap="8px">
        <ElButton type="warning" @click="handleResetPassword">{{
          $t("userManagement.detail.resetPassword")
        }}</ElButton>
        <ElButton
          type="info"
          :disabled="!admin?.mfa_enabled"
          @click="handleResetMfa"
        >
          {{ $t("userManagement.list.resetMfa") }}
        </ElButton>
        <ElButton
          v-if="admin?.role !== 'R_ADMIN'"
          type="danger"
          @click="handleDelete"
        >
          {{ $t("userManagement.detail.deleteUser") }}
        </ElButton>
      </CommonInlineRow>
    </ElCard>

    <ResetPasswordDialog
      v-model="resetPasswordVisible"
      :admin="admin"
      @success="loadData"
    />
  </div>
</template>

<script setup lang="ts">
import type { FormInstance, FormRules } from "element-plus";
import { ElMessageBox, ElMessage } from "element-plus";
import { $t } from "@/locales";
import type {
  AdminUser,
  UpdateAdminRequest,
  AdminStatus,
  AdminRole,
} from "@/types/api/admin";
import {
  fetchAdminById,
  updateAdmin,
  deleteAdmin,
  setAdminStatus,
  resetAdminMfa,
} from "@/api/admin";
import EditDialog from "@/components/common/EditDialog.vue";
import CommonInlineRow from "@/components/common/inline-row/index.vue";
import SurfaceCardHeader from "@/components/common/surface-card-header/index.vue";
import ResetPasswordDialog from "../modules/reset-password-dialog.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { logger } from "@/utils/sys";
import { getRoleType, getRoleLabel } from "@/utils/display-labels";
import { formatDateTime as formatDate } from "@/utils/format";
defineOptions({ name: "AdminDetail" });
const router = useRouter();
const route = useRoute();
const loading = ref(false);
const saving = ref(false);
const statusLoading = ref(false);
const editVisible = ref(false);
const admin = ref<AdminUser | null>(null);
const resetPasswordVisible = ref(false);
const formRef = ref<FormInstance>();
const editForm = reactive<UpdateAdminRequest>({
  email: "",
  role: "R_ADMIN",
  force_password_reset: false,
});
const rules: FormRules = {
  email: [
    {
      required: true,
      message: $t("userManagement.detail.validation.emailRequired"),
      trigger: "blur",
    },
    {
      type: "email",
      message: $t("userManagement.detail.validation.emailInvalid"),
      trigger: "blur",
    },
  ],
  role: [
    {
      required: true,
      message: $t("userManagement.detail.validation.roleRequired"),
      trigger: "change",
    },
  ],
};
const adminId = computed(() => route.params.id as string);
const loadData = async () => {
  try {
    await runWithLoading(loading, async () => {
      admin.value = await fetchAdminById(adminId.value);
      editForm.email = admin.value.email ?? undefined;
      editForm.role = admin.value.role;
      editForm.force_password_reset = admin.value.force_password_reset;
    });
  } catch (error) {
    logger.error("Failed to load admin:", error);
  }
};
const handleEdit = () => {
  if (!admin.value) return;
  editForm.email = admin.value.email ?? undefined;
  editForm.role = admin.value.role;
  editForm.force_password_reset = admin.value.force_password_reset;
  editVisible.value = true;
};
const handleSave = async () => {
  const valid = await formRef.value?.validate().catch(() => false);
  if (!valid) return;
  try {
    await runWithLoading(saving, async () => {
      admin.value = await updateAdmin(adminId.value, toRaw(editForm));
      ElMessage.success($t("common.feedback.saveSuccess"));
      editVisible.value = false;
    });
  } catch (error) {
    logger.error("Failed to update admin:", error);
  }
};
const handleStatusChange = async (active: string | number | boolean) => {
  const boolActive = Boolean(active);
  try {
    await runWithLoading(statusLoading, async () => {
      const status: AdminStatus = boolActive ? "active" : "inactive";
      await setAdminStatus(adminId.value, { status });
      ElMessage.success(
        boolActive
          ? $t("common.feedback.enabled")
          : $t("common.feedback.disabled"),
      );
      if (admin.value) {
        admin.value.is_active = boolActive;
      }
    });
  } catch (error) {
    logger.error("Failed to change status:", error);
  }
};
const handleResetPassword = () => {
  resetPasswordVisible.value = true;
};
const handleResetMfa = async () => {
  if (!admin.value) return;
  try {
    await ElMessageBox.confirm(
      $t("userManagement.messages.resetMfaConfirm", {
        username: admin.value.username,
      }),
      $t("userManagement.messages.resetMfaTitle"),
      {
        confirmButtonText: $t("common.confirm"),
        cancelButtonText: $t("common.cancel"),
        type: "warning",
      },
    );
    await resetAdminMfa(admin.value.id);
    ElMessage.success($t("common.feedback.mfaReset"));
    loadData();
  } catch (error) {
    if (error !== "cancel") {
      logger.error("Failed to reset MFA:", error);
    }
  }
};
const handleDelete = async () => {
  if (admin.value?.role === 'R_ADMIN') {
    return;
  }
  try {
    await ElMessageBox.confirm(
      $t("userManagement.messages.deleteUserConfirm", { username: admin.value?.username ?? "" }),
      $t("common.tips"),
      {
        confirmButtonText: $t("common.confirm"),
        cancelButtonText: $t("common.cancel"),
        type: "warning",
      },
    );
    await deleteAdmin(adminId.value);
    ElMessage.success($t("common.feedback.deleteSuccess"));
    router.push("/control-panel/users");
  } catch (error) {
    if (error !== "cancel") {
      logger.error("Failed to delete admin:", error);
    }
  }
};
onMounted(() => {
  loadData();
});
onActivated(() => {
  loadData();
});
</script>
