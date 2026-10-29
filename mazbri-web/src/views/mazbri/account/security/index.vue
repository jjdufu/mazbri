<template>
  <ControlPanelPageShell
    :title="$t('accountSecurity.page.title')"
    :subtitle="$t('accountSecurity.page.subtitle')"
    icon="ri:shield-user-line"
  >
    <div class="security-grid">
      <ElCard v-loading="statusLoading" shadow="hover" class="security-card">
        <template #header>
          <SurfaceCardHeader>
            <template #left>
              <div class="card-header-title">
                <ArtSvgIcon
                  icon="ri:smartphone-line"
                  class="card-header-icon"
                />
                <span>{{ $t("accountSecurity.mfa.title") }}</span>
              </div>
            </template>
            <template #actions>
              <ElTag :type="mfaEnabled ? 'success' : 'info'" effect="light">
                {{
                  mfaEnabled
                    ? $t("accountSecurity.mfa.statusEnabled")
                    : $t("accountSecurity.mfa.statusDisabled")
                }}
              </ElTag>
            </template>
          </SurfaceCardHeader>
        </template>

        <div class="settings-panel">
          <CommonInlineHint block size="xs">
            {{ $t("accountSecurity.mfa.description") }}
          </CommonInlineHint>
          <CommonInlineHint v-if="mfaEnabled" block size="xs">
            {{ $t("accountSecurity.mfa.freshHint") }}
          </CommonInlineHint>
          <CommonInlineHint v-if="mfaEnabled" block size="xs">
            {{ $t("accountSecurity.mfa.attemptsHint") }}
          </CommonInlineHint>
          <CommonInlineHint v-if="mfaEnabled" block size="xs">
            {{ $t("accountSecurity.mfa.lockHint") }}
          </CommonInlineHint>
          <CommonInlineHint v-if="mfaEnabled" block size="xs">
            {{ $t("accountSecurity.mfa.replayHint") }}
          </CommonInlineHint>

          <template v-if="mfaEnabled">
            <CommonInlineHint v-if="mfaRequired" block size="xs" tone="warning">
              {{ $t("accountSecurity.mfa.forcedHint") }}
            </CommonInlineHint>
            <CommonSettingRow
              mode="inline"
              mobile-stack
              :label="$t('accountSecurity.mfa.changedAt')"
            >
              <span class="setting-value">{{ changedAtText }}</span>
            </CommonSettingRow>
            <CommonSettingRow
              v-for="category in categoryList"
              :key="category"
              mode="inline"
              mobile-stack
              :label="$t(`accountSecurity.mfa.categories.${category}`)"
            >
              <span class="category-status">
                <span
                  class="setting-value"
                  :class="{ 'is-warning': categoryWarning(category) }"
                >
                  {{ categoryText(category) }}
                </span>
                <ElButton
                  v-if="isCategoryOpen(category)"
                  size="small"
                  :loading="endingCategory === category"
                  @click="confirmEndCategory(category)"
                >
                  {{ $t("accountSecurity.mfa.endCategory") }}
                </ElButton>
              </span>
            </CommonSettingRow>
            <CommonSettingRow
              mode="inline"
              mobile-stack
              :label="$t('accountSecurity.mfa.backupCodesRemaining')"
            >
              <span
                class="setting-value"
                :class="{ 'is-warning': backupCodesRemaining <= 2 }"
              >
                {{ backupCodesRemaining }}
              </span>
            </CommonSettingRow>
          </template>

          <!-- 绑定流程：获取密钥/二维码 → 输入验证码启用 -->
          <div v-if="!mfaEnabled && setupData" class="setup-section">
            <ol class="setup-steps">
              <li>{{ $t("accountSecurity.setup.stepInstall") }}</li>
              <li>{{ $t("accountSecurity.setup.stepScan") }}</li>
              <li>{{ $t("accountSecurity.setup.stepEnter") }}</li>
            </ol>
            <div class="setup-body">
              <div class="qr-wrap">
                <QrcodeVue
                  :value="setupData.qr_code_url"
                  :size="148"
                  level="M"
                  render-as="canvas"
                  background="#ffffff"
                  foreground="#000000"
                />
              </div>
              <div class="setup-fields">
                <div class="field-label">
                  {{ $t("accountSecurity.setup.secretLabel") }}
                </div>
                <div class="inline-field">
                  <ElInput :model-value="setupData.secret" readonly />
                  <ElButton @click="copyText(setupData.secret)">
                    {{ $t("accountSecurity.actions.copy") }}
                  </ElButton>
                </div>
                <div class="field-label">
                  {{
                    $t("accountSecurity.setup.codeLabel", {
                      digits: setupDigits,
                    })
                  }}
                </div>
                <div class="inline-field">
                  <ElInput
                    v-model.trim="setupCode"
                    :maxlength="setupDigits"
                    inputmode="numeric"
                    autocomplete="one-time-code"
                    :placeholder="$t('accountSecurity.setup.codePlaceholder')"
                    @keyup.enter="handleEnable"
                  />
                  <ElButton
                    type="primary"
                    :loading="actionLoading"
                    @click="handleEnable"
                  >
                    {{ $t("accountSecurity.actions.enable") }}
                  </ElButton>
                </div>
              </div>
            </div>
          </div>

          <div v-if="backupCodes.length > 0" class="backup-section">
            <div class="backup-header">
              <span class="field-label">
                {{ $t("accountSecurity.backupCodes.title") }}
              </span>
              <ElButton size="small" @click="copyText(backupCodes.join('\n'))">
                {{ $t("accountSecurity.actions.copyAll") }}
              </ElButton>
            </div>
            <CommonInlineHint block size="xs" tone="warning">
              {{ $t("accountSecurity.backupCodes.hint") }}
            </CommonInlineHint>
            <div class="backup-codes">
              <code v-for="code in backupCodes" :key="code">{{ code }}</code>
            </div>
          </div>

          <div class="card-actions">
            <template v-if="mfaEnabled">
              <ElButton :loading="actionLoading" @click="handleRegenerate">
                {{ $t("accountSecurity.actions.regenerateCodes") }}
              </ElButton>
              <ElButton
                v-if="!mfaRequired"
                type="danger"
                plain
                :loading="actionLoading"
                @click="handleDisable"
              >
                {{ $t("accountSecurity.actions.disable") }}
              </ElButton>
            </template>
            <template v-else-if="setupData">
              <ElButton @click="cancelSetup">
                {{ $t("common.cancel") }}
              </ElButton>
            </template>
            <ElButton
              v-else
              type="primary"
              :loading="actionLoading"
              @click="handleStartSetup"
            >
              {{ $t("accountSecurity.actions.bind") }}
            </ElButton>
          </div>
        </div>
      </ElCard>

      <ElCard shadow="hover" class="security-card">
        <template #header>
          <SurfaceCardHeader>
            <template #left>
              <div class="card-header-title">
                <ArtSvgIcon
                  icon="ri:lock-password-line"
                  class="card-header-icon"
                />
                <span>{{ $t("accountSecurity.password.title") }}</span>
              </div>
            </template>
          </SurfaceCardHeader>
        </template>

        <ElForm
          ref="passwordFormRef"
          :model="passwordForm"
          :rules="passwordRules"
          label-position="top"
          class="password-form"
          @submit.prevent="handleChangePassword"
        >
          <ElFormItem
            :label="$t('accountSecurity.password.current')"
            prop="current_password"
          >
            <ElInput
              v-model="passwordForm.current_password"
              type="password"
              show-password
              autocomplete="current-password"
            />
          </ElFormItem>
          <ElFormItem
            :label="$t('accountSecurity.password.new')"
            prop="new_password"
          >
            <ElInput
              v-model="passwordForm.new_password"
              type="password"
              show-password
              autocomplete="new-password"
            />
          </ElFormItem>
          <ElFormItem
            :label="$t('accountSecurity.password.confirm')"
            prop="confirm_password"
          >
            <ElInput
              v-model="passwordForm.confirm_password"
              type="password"
              show-password
              autocomplete="new-password"
            />
          </ElFormItem>
          <CommonInlineHint block size="xs">
            {{ $t("accountSecurity.password.policyHint") }}
          </CommonInlineHint>
          <div class="card-actions">
            <ElButton
              type="primary"
              native-type="submit"
              :loading="passwordLoading"
            >
              {{ $t("accountSecurity.actions.changePassword") }}
            </ElButton>
          </div>
        </ElForm>
      </ElCard>
    </div>
  </ControlPanelPageShell>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import {
  ElMessage,
  ElMessageBox,
  type FormInstance,
  type FormRules,
} from "element-plus";
import { storeToRefs } from "pinia";
import QrcodeVue from "qrcode.vue";
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import SurfaceCardHeader from "@/components/common/surface-card-header/index.vue";
import CommonSettingRow from "@/components/common/setting-row/index.vue";
import CommonInlineHint from "@/components/common/inline-hint/index.vue";
import {
  changePassword,
  disableMfa,
  enableMfa,
  fetchGetUserInfo,
  getMfaSetup,
  getMfaStatus,
  regenerateBackupCodes,
} from "@/api/auth";
import type {
  ChangePasswordRequest,
  MfaSessionRefresh,
  MfaSetupResponse,
  MfaStatus,
} from "@/types/api";
import { runWithLoading } from "@/composables/useAsyncState";
import { useUserStore } from "@/store/modules/user";
import { useMfaStepUpStore } from "@/store/modules/mfa-step-up";
import { copyTextToClipboard } from "@/utils/clipboard";
import {
  MFA_CATEGORIES,
  formatExpiryUtc8,
  formatMmSs,
  isWarning,
  readSessionExpiresIn,
  type MfaCategory,
} from "@/utils/http/mfa-step-up-state";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-dialog-service";
import { HttpError } from "@/utils/http/error";
import { $t } from "@/locales";
import { logger } from "@/utils/sys";

defineOptions({ name: "AccountSecurity" });

const userStore = useUserStore();
const stepUpStore = useMfaStepUpStore();
const { nowMs, windows } = storeToRefs(stepUpStore);
const categoryList = MFA_CATEGORIES;
const endingCategory = ref<MfaCategory | null>(null);
const statusLoading = ref(false);
const actionLoading = ref(false);
const passwordLoading = ref(false);
const status = ref<MfaStatus | null>(null);
const setupData = ref<MfaSetupResponse | null>(null);
const setupCode = ref("");
// 备用码只在绑定或重新生成后展示一次，离开页面即丢弃，不做任何持久化。
const backupCodes = ref<string[]>([]);

const mfaEnabled = computed(() => Boolean(status.value?.enabled));
const mfaRequired = computed(() => Boolean(status.value?.mfa_required));
const backupCodesRemaining = computed(
  () => status.value?.backup_codes_remaining ?? 0,
);
const changedAtText = computed(() => {
  const changedAt = status.value?.changed_at;
  if (!changedAt) return "-";
  return formatExpiryUtc8(changedAt) || "-";
});
function categoryRemaining(category: MfaCategory): number {
  void nowMs.value;
  return stepUpStore.remainingNow(category);
}
function isCategoryOpen(category: MfaCategory): boolean {
  return Boolean(windows.value[category]?.active) && categoryRemaining(category) > 0;
}
function categoryWarning(category: MfaCategory): boolean {
  return isCategoryOpen(category) && isWarning(categoryRemaining(category));
}
function categoryText(category: MfaCategory): string {
  if (!isCategoryOpen(category)) {
    return $t("accountSecurity.mfa.categoryInactive");
  }
  const time = formatMmSs(categoryRemaining(category));
  const statusText = $t(
    categoryWarning(category)
      ? "accountSecurity.mfa.categoryWarning"
      : "accountSecurity.mfa.categoryActive",
    { time },
  );
  const expiry = windows.value[category]?.expiresAt
    ? formatExpiryUtc8(windows.value[category].expiresAt ?? "")
    : "";
  return expiry
    ? `${statusText} · ${$t("accountSecurity.mfa.stepUpExpiry", { time: expiry })}`
    : statusText;
}
async function confirmEndCategory(category: MfaCategory): Promise<void> {
  const name = $t(`accountSecurity.mfa.categories.${category}`);
  try {
    await ElMessageBox.confirm(
      $t("accountSecurity.mfa.endCategoryConfirm", { category: name }),
      $t("accountSecurity.mfa.endCategoryTitle", { category: name }),
      { type: "warning" },
    );
  } catch {
    return;
  }
  endingCategory.value = category;
  try {
    await stepUpStore.endCategory(category);
  } catch (error) {
    ElMessage.error($t("accountSecurity.mfa.endCategoryFailed"));
    logActionError("Failed to end MFA category", error);
  } finally {
    endingCategory.value = null;
  }
}
function applyMfaSessionExpiry(payload: MfaSessionRefresh | void): void {
  if (!payload) return;
  const expiresIn = readSessionExpiresIn(payload);
  if (expiresIn != null) {
    userStore.setToken(payload.token, expiresIn);
  }
}
function logActionError(message: string, error: unknown): void {
  if (isMfaStepUpCancelled(error)) return;
  logger.error(message, error);
}
function isWindowUnavailable(error: unknown): boolean {
  if (!(error instanceof HttpError) || error.code !== 503) return false;
  const data = error.data;
  return (
    !!data &&
    typeof data === "object" &&
    (data as { code?: unknown }).code === "MFA_CHECK_UNAVAILABLE"
  );
}
const setupDigits = computed(() => setupData.value?.totp_digits ?? 6);

async function loadStatus(): Promise<void> {
  await runWithLoading(statusLoading, async () => {
    status.value = await getMfaStatus();
  }).catch((error) => {
    logger.error("Failed to load MFA status:", error);
  });
}

/** MFA 状态变化后同步顶部用户信息，供 MfaRequiredHint 等组件使用。 */
async function refreshUserInfo(): Promise<void> {
  try {
    userStore.setUserInfo(await fetchGetUserInfo());
  } catch (error) {
    logger.warn("Failed to refresh user info after MFA change:", error);
  }
}

async function copyText(text: string): Promise<void> {
  const ok = await copyTextToClipboard(text);
  if (ok) {
    ElMessage.success($t("accountSecurity.messages.copied"));
  } else {
    ElMessage.error($t("accountSecurity.messages.copyFailed"));
  }
}

async function handleStartSetup(): Promise<void> {
  let currentPassword = "";
  try {
    const { value } = await ElMessageBox.prompt(
      $t("accountSecurity.setup.passwordMessage"),
      $t("accountSecurity.setup.passwordTitle"),
      {
        inputType: "password",
        inputPlaceholder: $t("accountSecurity.setup.passwordPlaceholder"),
        inputValidator: (input: string) =>
          input.trim().length > 0 ||
          $t("accountSecurity.setup.passwordRequired"),
        confirmButtonText: $t("common.confirm"),
        cancelButtonText: $t("common.cancel"),
        closeOnClickModal: false,
      },
    );
    currentPassword = String(value);
  } catch {
    return;
  }
  await runWithLoading(actionLoading, async () => {
    const data = await getMfaSetup(currentPassword);
    setupData.value = data;
    setupCode.value = "";
    // 绑定阶段生成的备用码在启用成功后才生效，先展示让用户保存。
    backupCodes.value = data.backup_codes ?? [];
  }).catch((error) => {
    logActionError("Failed to start MFA setup:", error);
  });
}

function cancelSetup(): void {
  setupData.value = null;
  setupCode.value = "";
  backupCodes.value = [];
}

async function handleEnable(): Promise<void> {
  const code = setupCode.value;
  if (!new RegExp(`^\\d{${setupDigits.value}}$`).test(code)) {
    ElMessage.warning(
      $t("accountSecurity.setup.codeInvalid", { digits: setupDigits.value }),
    );
    return;
  }
  await runWithLoading(actionLoading, async () => {
    const enabled = await enableMfa({ totp_code: code });
    applyMfaSessionExpiry(enabled);
    setupData.value = null;
    setupCode.value = "";
    await Promise.all([
      loadStatus(),
      refreshUserInfo(),
      stepUpStore.refresh().catch(() => undefined),
    ]);
  }).catch(async (error) => {
    if (isWindowUnavailable(error)) {
      await Promise.all([
        loadStatus(),
        refreshUserInfo(),
        stepUpStore.refresh().catch(() => undefined),
      ]);
    }
    logActionError("Failed to enable MFA:", error);
  });
}

async function handleRegenerate(): Promise<void> {
  try {
    await ElMessageBox.confirm(
      $t("accountSecurity.backupCodes.regenerateConfirm"),
      $t("accountSecurity.actions.regenerateCodes"),
      { type: "warning" },
    );
  } catch {
    return;
  }
  await runWithLoading(actionLoading, async () => {
    const result = await regenerateBackupCodes();
    backupCodes.value = result.backup_codes ?? [];
    await loadStatus();
  }).catch((error) => {
    logActionError("Failed to regenerate backup codes:", error);
  });
}

async function handleDisable(): Promise<void> {
  try {
    await ElMessageBox.confirm(
      $t("accountSecurity.disable.message"),
      $t("accountSecurity.disable.title"),
      {
        type: "warning",
        confirmButtonText: $t("accountSecurity.actions.disable"),
        cancelButtonText: $t("common.cancel"),
        closeOnClickModal: false,
      },
    );
  } catch {
    return;
  }
  await runWithLoading(actionLoading, async () => {
    const refreshed = await disableMfa();
    applyMfaSessionExpiry(refreshed);
    backupCodes.value = [];
    await Promise.all([
      loadStatus(),
      refreshUserInfo(),
      stepUpStore.refresh().catch(() => undefined),
    ]);
  }).catch((error) => {
    logActionError("Failed to disable MFA:", error);
  });
}

const passwordFormRef = ref<FormInstance>();
const passwordForm = reactive<ChangePasswordRequest>({
  current_password: "",
  new_password: "",
  confirm_password: "",
});
const passwordRules: FormRules<ChangePasswordRequest> = {
  current_password: [
    {
      required: true,
      message: $t("accountSecurity.password.currentRequired"),
      trigger: "blur",
    },
  ],
  new_password: [
    {
      required: true,
      message: $t("accountSecurity.password.newRequired"),
      trigger: "blur",
    },
  ],
  confirm_password: [
    {
      required: true,
      message: $t("accountSecurity.password.confirmRequired"),
      trigger: "blur",
    },
    {
      validator: (_rule, value, callback) => {
        if (value && value !== passwordForm.new_password) {
          callback(new Error($t("accountSecurity.password.mismatch")));
        } else {
          callback();
        }
      },
      trigger: "blur",
    },
  ],
};

async function handleChangePassword(): Promise<void> {
  const valid = await passwordFormRef.value?.validate().catch(() => false);
  if (!valid) return;
  await runWithLoading(passwordLoading, async () => {
    await changePassword({ ...passwordForm });
    passwordFormRef.value?.resetFields();
  }).catch((error) => {
    logger.error("Failed to change password:", error);
  });
}

onMounted(() => {
  void loadStatus();
  void stepUpStore.refresh().catch(() => undefined);
});
</script>

<style scoped lang="scss">
.security-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 16px;
  align-items: start;
}

.security-card {
  min-width: 0;

  :deep(.el-card__header) {
    padding: 20px 20px 0 !important;
    border-bottom: none !important;
  }

  :deep(.el-card__body) {
    padding: 12px 20px 20px !important;
  }
}

.card-header-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 15px;
  font-weight: 600;
}

.card-header-icon {
  font-size: 18px;
  color: var(--el-color-primary);
}

// 卡片内部为扁平分区，不再嵌套带边框的盒子。
.settings-panel {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

:deep(.common-setting-row__label) {
  width: 140px;
  font-size: 13px;
  font-weight: 500;
}

:deep(.common-setting-row__control) {
  display: flex;
  justify-content: flex-end;
}

.category-status {
  display: inline-flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
  flex-wrap: wrap;
}

.setting-value {
  font-size: 13px;
  color: var(--el-text-color-primary);

  &.is-warning {
    color: var(--el-color-warning);
  }
}

.field-label {
  font-size: 13px;
  font-weight: 500;
  color: var(--el-text-color-regular);
}

.setup-section {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.setup-steps {
  margin: 0;
  padding-left: 18px;
  font-size: 13px;
  line-height: 1.7;
  color: var(--el-text-color-secondary);
  list-style: decimal;
}

.setup-body {
  display: flex;
  gap: 16px;
  align-items: flex-start;
}

.qr-wrap {
  flex-shrink: 0;
  padding: 8px;
  background: #ffffff;
  border-radius: var(--art-form-control-radius);
}

.setup-fields {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}

.inline-field {
  display: flex;
  gap: 8px;
}

.backup-section {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.backup-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.backup-codes {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
  gap: 6px 16px;

  code {
    font-family: var(--el-font-family-mono, ui-monospace, monospace);
    font-size: 13px;
    letter-spacing: 0.5px;
    color: var(--el-text-color-primary);
  }
}

.card-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  justify-content: flex-end;
  padding-top: 4px;
}

.password-form {
  display: flex;
  flex-direction: column;

  :deep(.el-form-item) {
    margin-bottom: 14px;
  }
}

@media (max-width: 1100px) {
  .security-grid {
    grid-template-columns: 1fr;
  }
}

@media (max-width: 640px) {
  .setup-body {
    flex-direction: column;
  }
}
</style>
