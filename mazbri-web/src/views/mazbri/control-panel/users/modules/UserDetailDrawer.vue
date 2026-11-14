<template>
  <AdaptiveDrawer v-model="visible" size="560px" destroy-on-close>
    <template #header>
      <CommonDrawerHeader
        :title="headerTitle"
        :subtitle="headerSubtitle"
        align="center"
      >
        <template #actions>
          <ElButton
            type="primary"
            plain
            :disabled="!user"
            @click="editVisible = true"
          >
            {{ $t("userManagement.detail.editAction") }}
          </ElButton>
        </template>
      </CommonDrawerHeader>
    </template>

    <div v-loading="loading" class="user-detail-drawer">
      <template v-if="user">
        <section class="drawer-section">
          <div class="drawer-section__title">
            {{ $t("userManagement.detail.basicInfo") }}
          </div>
          <ElDescriptions :column="2" border size="small">
            <ElDescriptionsItem
              :label="$t('userManagement.detail.labels.id')"
              :span="2"
            >
              <CopyableText
                :value="user.id"
                :label="$t('userManagement.detail.labels.id')"
              />
            </ElDescriptionsItem>
            <ElDescriptionsItem
              :label="$t('userManagement.detail.labels.username')"
            >
              <CopyableText
                :value="user.username"
                :label="$t('userManagement.detail.labels.username')"
              />
            </ElDescriptionsItem>
            <ElDescriptionsItem
              :label="$t('userManagement.detail.labels.displayName')"
            >
              {{ user.display_name || EMPTY_PLACEHOLDER }}
            </ElDescriptionsItem>
            <ElDescriptionsItem
              :label="$t('userManagement.detail.labels.email')"
              :span="2"
            >
              {{ user.email || EMPTY_PLACEHOLDER }}
            </ElDescriptionsItem>
            <ElDescriptionsItem
              :label="$t('userManagement.detail.labels.role')"
            >
              <ElTag :type="getRoleType(user.role)" effect="light" size="small">
                {{ getRoleLabel(user.role) }}
              </ElTag>
            </ElDescriptionsItem>
            <ElDescriptionsItem
              :label="$t('userManagement.detail.labels.status')"
            >
              <ElTag
                :type="user.is_active ? 'success' : 'danger'"
                effect="light"
                size="small"
              >
                {{
                  user.is_active
                    ? $t("userManagement.status.active")
                    : $t("userManagement.status.inactive")
                }}
              </ElTag>
            </ElDescriptionsItem>
            <ElDescriptionsItem
              :label="$t('userManagement.detail.labels.mfaStatus')"
            >
              <ElTag
                :type="user.mfa_enabled ? 'success' : 'info'"
                :effect="user.mfa_enabled ? 'light' : 'plain'"
                size="small"
              >
                {{
                  user.mfa_enabled
                    ? $t("userManagement.status.mfaBound")
                    : $t("userManagement.status.mfaUnbound")
                }}
              </ElTag>
            </ElDescriptionsItem>
            <ElDescriptionsItem
              :label="$t('userManagement.detail.labels.mfaRequired')"
            >
              <ElTag
                :type="user.mfa_required ? 'warning' : 'info'"
                :effect="user.mfa_required ? 'light' : 'plain'"
                size="small"
              >
                {{
                  user.mfa_required
                    ? $t("userManagement.status.policyOn")
                    : $t("userManagement.status.policyOff")
                }}
              </ElTag>
            </ElDescriptionsItem>
            <ElDescriptionsItem
              :label="$t('userManagement.detail.labels.forcePasswordReset')"
            >
              <ElTag
                :type="user.force_password_reset ? 'warning' : 'info'"
                :effect="user.force_password_reset ? 'light' : 'plain'"
                size="small"
              >
                {{
                  user.force_password_reset
                    ? $t("userManagement.status.policyOn")
                    : $t("userManagement.status.policyOff")
                }}
              </ElTag>
            </ElDescriptionsItem>
            <ElDescriptionsItem
              :label="$t('userManagement.detail.labels.passwordChangedAt')"
            >
              {{
                user.password_changed_at
                  ? formatDate(user.password_changed_at)
                  : EMPTY_PLACEHOLDER
              }}
            </ElDescriptionsItem>
            <ElDescriptionsItem
              :label="$t('userManagement.detail.labels.lastLogin')"
            >
              {{
                user.last_login_at
                  ? formatDate(user.last_login_at)
                  : EMPTY_PLACEHOLDER
              }}
            </ElDescriptionsItem>
            <ElDescriptionsItem :label="$t('common.fields.createdAt')">
              {{ formatDate(user.created_at) }}
            </ElDescriptionsItem>
            <ElDescriptionsItem
              :label="$t('common.fields.updatedAt')"
              :span="2"
            >
              {{ formatDate(user.updated_at) }}
            </ElDescriptionsItem>
          </ElDescriptions>
        </section>

        <section class="drawer-section">
          <div class="drawer-section__title">
            {{ $t("userManagement.detail.securitySection") }}
          </div>
          <div class="security-control-list">
            <div class="security-control-row">
              <div class="security-control-row__meta">
                <div class="security-control-row__label">
                  {{ $t("userManagement.detail.controls.statusTitle") }}
                </div>
                <div class="security-control-row__hint">
                  {{
                    isSuperAdminTarget || isSelf
                      ? $t("userManagement.detail.controls.statusProtectedHint")
                      : $t("userManagement.detail.controls.statusHint")
                  }}
                </div>
              </div>
              <ElSwitch
                :model-value="user.is_active"
                :loading="statusLoading"
                :disabled="isSuperAdminTarget || isSelf"
                @change="handleStatusChange"
              />
            </div>

            <div class="security-control-row">
              <div class="security-control-row__meta">
                <div class="security-control-row__label">
                  {{ $t("userManagement.detail.controls.passwordTitle") }}
                </div>
                <div class="security-control-row__hint">
                  {{
                    isSuperAdminTarget
                      ? $t(
                          "userManagement.detail.controls.passwordProtectedHint",
                        )
                      : $t("userManagement.detail.controls.passwordHint")
                  }}
                </div>
              </div>
              <ElButton
                type="warning"
                plain
                :disabled="isSuperAdminTarget"
                @click="resetPasswordVisible = true"
              >
                {{ $t("userManagement.detail.resetPassword") }}
              </ElButton>
            </div>

            <div class="security-control-row">
              <div class="security-control-row__meta">
                <div class="security-control-row__label">
                  {{ $t("userManagement.detail.controls.mfaTitle") }}
                </div>
                <div class="security-control-row__hint">
                  {{ mfaControlHint }}
                </div>
              </div>
              <ElButton
                type="info"
                plain
                :loading="mfaLoading"
                :disabled="isSuperAdminTarget || !user.mfa_enabled"
                @click="handleResetMfa"
              >
                {{ $t("userManagement.detail.resetMfa") }}
              </ElButton>
            </div>

            <div
              v-if="!isSuperAdminTarget && !isSelf"
              class="security-control-row security-control-row--danger"
            >
              <div class="security-control-row__meta">
                <div class="security-control-row__label">
                  {{ $t("userManagement.detail.controls.dangerTitle") }}
                </div>
                <div class="security-control-row__hint">
                  {{ $t("userManagement.detail.controls.dangerHint") }}
                </div>
              </div>
              <ElButton
                type="danger"
                plain
                :loading="deleteLoading"
                @click="handleDelete"
              >
                {{ $t("userManagement.detail.deleteUser") }}
              </ElButton>
            </div>
          </div>
        </section>
      </template>

      <UserEditDialog
        v-if="editVisible"
        v-model="editVisible"
        :admin="user"
        @success="handleEditSuccess"
      />

      <ResetPasswordDialog
        v-if="resetPasswordVisible"
        v-model="resetPasswordVisible"
        :admin="user"
        @success="handleChanged"
      />
    </div>
  </AdaptiveDrawer>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { ElMessage, ElMessageBox } from "element-plus";
import { useI18n } from "vue-i18n";
import { $t } from "@/locales";
import type { AdminUser } from "@/types/api/admin";
import {
  deleteAdmin,
  fetchAdminById,
  resetAdminMfa,
  setAdminStatus,
} from "@/api/admin";
import AdaptiveDrawer from "@/components/common/AdaptiveDrawer.vue";
import CommonDrawerHeader from "@/components/common/drawer-header/index.vue";
import CopyableText from "@/components/common/copyable-text/index.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { useUserStore } from "@/store/modules/user";
import { getRoleLabel, getRoleType } from "@/utils/display-labels";
import {
  EMPTY_PLACEHOLDER,
  formatDateTime as formatDate,
} from "@/utils/format";
import { logger } from "@/utils/sys";
import ResetPasswordDialog from "./reset-password-dialog.vue";
import UserEditDialog from "./user-edit-dialog.vue";

defineOptions({ name: "UserDetailDrawer" });

const visible = defineModel<boolean>({ required: true });
const props = defineProps<{
  adminId: string | null;
  initialAdmin?: AdminUser | null;
}>();
const emit = defineEmits<{
  changed: [];
  deleted: [];
}>();

const { t } = useI18n();
const userStore = useUserStore();
const loading = ref(false);
const statusLoading = ref(false);
const mfaLoading = ref(false);
const deleteLoading = ref(false);
const editVisible = ref(false);
const resetPasswordVisible = ref(false);
const user = ref<AdminUser | null>(null);

const isSuperAdminTarget = computed(() => user.value?.role === "R_ADMIN");
const isSelf = computed(
  () =>
    Boolean(user.value?.id) &&
    user.value?.id === String(userStore.getUserInfo.id ?? ""),
);

const headerTitle = computed(
  () =>
    user.value?.display_name ||
    user.value?.username ||
    t("userManagement.detail.userTitle"),
);
const headerSubtitle = computed(
  () => user.value?.email || user.value?.username || "",
);

const mfaControlHint = computed(() => {
  if (isSuperAdminTarget.value) {
    return t("userManagement.detail.controls.mfaProtectedHint");
  }
  if (!user.value?.mfa_enabled) {
    return t("userManagement.detail.controls.mfaUnboundHint");
  }
  return t("userManagement.detail.controls.mfaHint");
});

async function loadUser(): Promise<void> {
  if (!props.adminId) return;
  try {
    await runWithLoading(loading, async () => {
      user.value = await fetchAdminById(props.adminId as string);
    });
  } catch (error) {
    logger.error("Failed to load admin detail:", error);
  }
}

function handleEditSuccess(updated: AdminUser): void {
  user.value = updated;
  emit("changed");
}

async function handleChanged(): Promise<void> {
  await loadUser();
  emit("changed");
}

async function handleStatusChange(active: string | number | boolean): Promise<void> {
  if (!user.value || isSuperAdminTarget.value || isSelf.value) return;
  const boolActive = Boolean(active);
  try {
    await runWithLoading(statusLoading, async () => {
      await setAdminStatus(user.value!.id, { is_active: boolActive });
      if (user.value) {
        user.value.is_active = boolActive;
      }
      ElMessage.success(
        boolActive
          ? $t("common.feedback.enabled")
          : $t("common.feedback.disabled"),
      );
      emit("changed");
    });
  } catch (error) {
    logger.error("Failed to change admin status:", error);
  }
}

async function handleResetMfa(): Promise<void> {
  if (!user.value || isSuperAdminTarget.value || !user.value.mfa_enabled) return;
  try {
    await ElMessageBox.confirm(
      $t("userManagement.messages.resetMfaConfirm", {
        username: user.value.username,
      }),
      $t("userManagement.messages.resetMfaTitle"),
      {
        confirmButtonText: $t("common.confirm"),
        cancelButtonText: $t("common.cancel"),
        type: "warning",
      },
    );
    await runWithLoading(mfaLoading, async () => {
      await resetAdminMfa(user.value!.id);
      ElMessage.success($t("common.feedback.mfaReset"));
      await loadUser();
      emit("changed");
    });
  } catch (error) {
    if (error !== "cancel") {
      logger.error("Failed to reset MFA:", error);
    }
  }
}

async function handleDelete(): Promise<void> {
  if (!user.value || isSuperAdminTarget.value || isSelf.value) return;
  try {
    await ElMessageBox.confirm(
      $t("userManagement.messages.deleteUserConfirm", {
        username: user.value.username,
      }),
      $t("common.tips"),
      {
        confirmButtonText: $t("common.confirm"),
        cancelButtonText: $t("common.cancel"),
        type: "warning",
      },
    );
    await runWithLoading(deleteLoading, async () => {
      await deleteAdmin(user.value!.id);
      ElMessage.success($t("common.feedback.deleteSuccess"));
      visible.value = false;
      emit("deleted");
    });
  } catch (error) {
    if (error !== "cancel") {
      logger.error("Failed to delete user:", error);
    }
  }
}

watch(
  () => [visible.value, props.adminId] as const,
  ([isOpen, id]) => {
    if (!isOpen) {
      editVisible.value = false;
      resetPasswordVisible.value = false;
      return;
    }
    if (props.initialAdmin && props.initialAdmin.id === id) {
      user.value = { ...props.initialAdmin };
    }
    if (id) {
      void loadUser();
    }
  },
  { immediate: true },
);
</script>

<style scoped lang="scss">
.user-detail-drawer {
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.drawer-section {
  display: flex;
  flex-direction: column;
  gap: 10px;

  &__title {
    font-size: 14px;
    font-weight: 600;
    color: var(--el-text-color-primary);
  }
}

.security-control-list {
  display: flex;
  flex-direction: column;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: var(--el-border-radius-base);
  background-color: var(--el-bg-color);
}

.security-control-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 12px 14px;

  & + & {
    border-top: 1px solid var(--el-border-color-lighter);
  }

  &__meta {
    flex: 1;
    min-width: 0;
  }

  &__label {
    font-size: 13px;
    font-weight: 500;
    color: var(--el-text-color-primary);
  }

  &__hint {
    margin-top: 2px;
    font-size: 12px;
    line-height: 1.5;
    color: var(--el-text-color-secondary);
  }
}
</style>
