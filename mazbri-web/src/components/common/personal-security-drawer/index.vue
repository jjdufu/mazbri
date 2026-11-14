<template>
  <AdaptiveDrawer
    v-model="personalSecurityStore.drawerVisible"
    size="520px"
    destroy-on-close
  >
    <template #header>
      <CommonDrawerHeader
        :title="$t('accountSecurity.drawer.title')"
        :subtitle="drawerSubtitle"
      />
    </template>

    <div class="personal-security-drawer">
      <div ref="mfaSectionRef" class="drawer-section">
        <SettingsCard
          v-loading="statusLoading"
          icon="ri:smartphone-line"
          :title="$t('accountSecurity.mfa.title')"
          :info="$t('accountSecurity.mfa.hint')"
        >
          <template #title-extra>
            <ElTag
              :type="mfaEnabled ? 'success' : 'info'"
              effect="light"
              size="small"
            >
              {{
                mfaEnabled
                  ? $t("accountSecurity.mfa.statusEnabled")
                  : $t("accountSecurity.mfa.statusDisabled")
              }}
            </ElTag>
          </template>
          <template #actions>
            <template v-if="mfaEnabled">
              <ElButton :loading="actionLoading" @click="handleRegenerate">
                {{ $t("accountSecurity.actions.regenerateCodes") }}
              </ElButton>
              <ElButton
                v-if="isAdmin && !mfaRequired"
                type="danger"
                plain
                :loading="actionLoading"
                @click="handleDisable"
              >
                {{ $t("accountSecurity.actions.disable") }}
              </ElButton>
            </template>
            <ElButton v-else-if="setupData" @click="cancelSetup">
              {{ $t("common.cancel") }}
            </ElButton>
            <ElButton
              v-else
              type="primary"
              :loading="actionLoading"
              @click="handleStartSetup"
            >
              {{ $t("accountSecurity.actions.bind") }}
            </ElButton>
          </template>

          <template v-if="mfaEnabled">
            <CommonSettingRow
              mode="inline"
              mobile-stack
              :label="$t('accountSecurity.mfa.changedAt')"
            >
              <span class="setting-value">{{ changedAtText }}</span>
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
            <p v-if="mfaRequired || !isAdmin" class="mfa-lock-hint">
              {{
                mfaRequired
                  ? $t("accountSecurity.mfa.forcedHint")
                  : $t("accountSecurity.mfa.userHint")
              }}
            </p>
          </template>

          <template v-else-if="!setupData">
            <CommonSettingRow
              mode="inline"
              mobile-stack
              :label="$t('accountSecurity.mfa.specLabel')"
            >
              <span class="setting-value">{{
                $t("accountSecurity.mfa.specDigitsValue", { digits: setupDigits })
              }}</span>
            </CommonSettingRow>
            <CommonSettingRow
              mode="inline"
              mobile-stack
              :label="$t('accountSecurity.mfa.changedAt')"
            >
              <span class="setting-value">{{ changedAtText }}</span>
            </CommonSettingRow>
          </template>

          <template v-if="!mfaEnabled && setupData">
            <div class="setup-body">
              <div class="qr-wrap">
                <QrcodeVue
                  :value="setupData.qr_code_url"
                  :size="136"
                  level="M"
                  render-as="canvas"
                  background="#ffffff"
                  foreground="#000000"
                />
              </div>
              <div class="setup-fields">
                <CommonSettingRow
                  mode="block"
                  :label="$t('accountSecurity.setup.secretLabel')"
                >
                  <div class="inline-field">
                    <ElInput :model-value="setupData.secret" readonly />
                    <ElButton @click="copyText(setupData.secret)">
                      {{ $t("accountSecurity.actions.copy") }}
                    </ElButton>
                  </div>
                </CommonSettingRow>
                <CommonSettingRow
                  mode="block"
                  :label="
                    $t('accountSecurity.setup.codeLabel', { digits: setupDigits })
                  "
                >
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
                </CommonSettingRow>
              </div>
            </div>
          </template>

          <CommonSettingRow
            v-if="backupCodes.length > 0"
            mode="block"
            :label="$t('accountSecurity.backupCodes.title')"
          >
            <div class="backup-block">
              <p class="backup-hint">
                {{ $t("accountSecurity.backupCodes.hint") }}
              </p>
              <div class="backup-codes">
                <code v-for="code in backupCodes" :key="code">{{ code }}</code>
              </div>
              <div>
                <ElButton @click="copyText(backupCodes.join('\n'))">
                  {{ $t("accountSecurity.actions.copyAll") }}
                </ElButton>
              </div>
            </div>
          </CommonSettingRow>
        </SettingsCard>
      </div>

      <div ref="passwordSectionRef" class="drawer-section">
        <SettingsCard
          icon="ri:lock-password-line"
          :title="$t('accountSecurity.password.title')"
          :info="passwordPolicyHintText"
        >
          <CommonSettingRow
            mode="block"
            :label="$t('accountSecurity.password.current')"
          >
            <ElInput
              v-model="passwordForm.current_password"
              type="password"
              show-password
              autocomplete="current-password"
              @keyup.enter="handleChangePassword"
            />
          </CommonSettingRow>
          <CommonSettingRow
            mode="block"
            :label="$t('accountSecurity.password.new')"
          >
            <ElInput
              v-model="passwordForm.new_password"
              type="password"
              show-password
              autocomplete="new-password"
              @keyup.enter="handleChangePassword"
            />
          </CommonSettingRow>
          <CommonSettingRow
            mode="block"
            :label="$t('accountSecurity.password.confirm')"
          >
            <ElInput
              v-model="passwordForm.confirm_password"
              type="password"
              show-password
              autocomplete="new-password"
              @keyup.enter="handleChangePassword"
            />
          </CommonSettingRow>

          <div class="password-actions">
            <ElButton
              v-if="hasPasswordInput"
              :disabled="passwordLoading"
              @click="resetPasswordForm"
            >
              {{ $t("common.reset") }}
            </ElButton>
            <ElButton
              type="primary"
              :loading="passwordLoading"
              @click="handleChangePassword"
            >
              {{ $t("accountSecurity.actions.confirmChangePassword") }}
            </ElButton>
          </div>
        </SettingsCard>
      </div>
    </div>
  </AdaptiveDrawer>
</template>

<script setup lang="ts">
import { computed, nextTick, reactive, ref, watch } from "vue";
import { ElMessage, ElMessageBox } from "element-plus";
import QrcodeVue from "qrcode.vue";
import AdaptiveDrawer from "@/components/common/AdaptiveDrawer.vue";
import CommonDrawerHeader from "@/components/common/drawer-header/index.vue";
import SettingsCard from "@/components/common/settings-card/index.vue";
import CommonSettingRow from "@/components/common/setting-row/index.vue";
import {
  changePassword,
  disableMfa,
  enableMfa,
  fetchGetUserInfo,
  getMfaSetup,
  getMfaStatus,
  regenerateBackupCodes,
} from "@/api/auth";
import { fetchPasswordPolicy } from "@/api/settings";
import type {
  ChangePasswordRequest,
  MfaSessionRefresh,
  MfaSetupResponse,
  MfaStatus,
  PasswordPolicy,
} from "@/types/api";
import { runWithLoading } from "@/composables/useAsyncState";
import { useOpsRole } from "@/composables/ops/useOpsRole";
import { useUserStore } from "@/store/modules/user";
import { useMfaStepUpStore } from "@/store/modules/mfa-step-up";
import { usePersonalSecurityStore } from "@/store/modules/personal-security";
import { copyWithFeedback } from "@/utils/ui/copy-feedback";
import {
  formatExpiryUtc8,
  readSessionExpiresIn,
} from "@/utils/http/mfa-step-up-state";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-dialog-service";
import { HttpError } from "@/utils/http/error";
import { resolvePasswordPolicyPayload } from "@/utils/password";
import { $t } from "@/locales";
import { logger } from "@/utils/sys";

defineOptions({ name: "PersonalSecurityDrawer" });

const personalSecurityStore = usePersonalSecurityStore();
const userStore = useUserStore();
const stepUpStore = useMfaStepUpStore();
const { isAdmin } = useOpsRole();

const mfaSectionRef = ref<HTMLElement | null>(null);
const passwordSectionRef = ref<HTMLElement | null>(null);
const statusLoading = ref(false);
const actionLoading = ref(false);
const passwordLoading = ref(false);
const status = ref<MfaStatus | null>(null);
const setupData = ref<MfaSetupResponse | null>(null);
const setupCode = ref("");
const backupCodes = ref<string[]>([]);
const passwordPolicy = ref<PasswordPolicy | null>(null);

const passwordForm = reactive<ChangePasswordRequest>({
  current_password: "",
  new_password: "",
  confirm_password: "",
});

const drawerSubtitle = computed(() => {
  const username = userStore.info?.username;
  return username
    ? $t("accountSecurity.drawer.subtitleWithUser", { username })
    : $t("accountSecurity.drawer.subtitle");
});

const mfaEnabled = computed(() => Boolean(status.value?.enabled));
const mfaRequired = computed(() => Boolean(status.value?.mfa_required));
const backupCodesRemaining = computed(
  () => status.value?.backup_codes_remaining ?? 0,
);
const changedAtText = computed(() => {
  const changedAt = status.value?.changed_at;
  if (!changedAt) return "—";
  return formatExpiryUtc8(changedAt) || "—";
});
const setupDigits = computed(() => {
  const digits = setupData.value?.totp_digits ?? status.value?.totp_digits ?? 6;
  return digits === 8 ? 8 : 6;
});

const hasPasswordInput = computed(
  () =>
    Boolean(passwordForm.current_password) ||
    Boolean(passwordForm.new_password) ||
    Boolean(passwordForm.confirm_password),
);

const passwordPolicyHintText = computed(() => {
  const p = passwordPolicy.value;
  if (!p) {
    return $t("accountSecurity.password.policyHint");
  }
  const requiredTypes: string[] = [];
  if (p.require_uppercase) {
    requiredTypes.push($t("accountSecurity.password.summary.uppercase"));
  }
  if (p.require_lowercase) {
    requiredTypes.push($t("accountSecurity.password.summary.lowercase"));
  }
  if (p.require_digit) {
    requiredTypes.push($t("accountSecurity.password.summary.digit"));
  }
  if (p.require_special_char) {
    requiredTypes.push(
      $t("accountSecurity.password.summary.special", {
        chars: p.special_chars || "!@#$%^&*",
      }),
    );
  }
  const lengthPart = $t("accountSecurity.password.summary.length", {
    min: p.min_length,
    max: p.max_length,
  });
  const typesPart =
    requiredTypes.length > 0
      ? $t("accountSecurity.password.summary.charTypes", {
          types: requiredTypes.join(" / "),
        })
      : $t("accountSecurity.password.summary.charTypesNone");
  return `${lengthPart} · ${typesPart}`;
});

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

async function loadStatus(): Promise<void> {
  await runWithLoading(statusLoading, async () => {
    status.value = await getMfaStatus();
  }).catch((error) => {
    logger.error("Failed to load MFA status:", error);
  });
}

async function loadPasswordPolicy(): Promise<void> {
  try {
    const response = await fetchPasswordPolicy();
    passwordPolicy.value = resolvePasswordPolicyPayload(response);
  } catch (err) {
    logger.warn("Failed to load password policy summary:", err);
  }
}

async function refreshUserInfo(): Promise<void> {
  try {
    userStore.setUserInfo(await fetchGetUserInfo());
  } catch (error) {
    logger.warn("Failed to refresh user info after MFA change:", error);
  }
}

async function refreshAfterMfaChange(): Promise<void> {
  await Promise.all([
    loadStatus(),
    refreshUserInfo(),
    stepUpStore.refresh().catch(() => undefined),
  ]);
}

async function copyText(text: string): Promise<void> {
  await copyWithFeedback(text);
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
    await refreshAfterMfaChange();
  }).catch(async (error) => {
    if (isWindowUnavailable(error)) {
      await refreshAfterMfaChange();
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
  if (!isAdmin.value || mfaRequired.value) return;
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
    await refreshAfterMfaChange();
  }).catch((error) => {
    logActionError("Failed to disable MFA:", error);
  });
}

function validatePasswordForm(): string | null {
  if (!passwordForm.current_password.trim()) {
    return $t("accountSecurity.password.currentRequired");
  }
  if (!passwordForm.new_password.trim()) {
    return $t("accountSecurity.password.newRequired");
  }
  if (!passwordForm.confirm_password.trim()) {
    return $t("accountSecurity.password.confirmRequired");
  }
  if (passwordForm.confirm_password !== passwordForm.new_password) {
    return $t("accountSecurity.password.mismatch");
  }
  return null;
}

function resetPasswordForm(): void {
  passwordForm.current_password = "";
  passwordForm.new_password = "";
  passwordForm.confirm_password = "";
}

async function handleChangePassword(): Promise<void> {
  const error = validatePasswordForm();
  if (error) {
    ElMessage.warning(error);
    return;
  }
  await runWithLoading(passwordLoading, async () => {
    await changePassword({ ...passwordForm });
    resetPasswordForm();
  }).catch((err) => {
    logger.error("Failed to change password:", err);
  });
}

watch(
  () => personalSecurityStore.drawerVisible,
  (visible) => {
    if (!visible) {
      setupData.value = null;
      setupCode.value = "";
      backupCodes.value = [];
      resetPasswordForm();
      return;
    }
    void loadStatus();
    void loadPasswordPolicy();
    void stepUpStore.refresh().catch(() => undefined);
    void nextTick(() => {
      const target =
        personalSecurityStore.focusSection === "password"
          ? passwordSectionRef.value
          : mfaSectionRef.value;
      target?.scrollIntoView({ behavior: "smooth", block: "start" });
    });
  },
  { immediate: true },
);
</script>

<style scoped lang="scss">
.personal-security-drawer {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.drawer-section {
  min-width: 0;
}

.setting-value {
  font-size: 14px;
  color: var(--el-text-color-primary);

  &.is-warning {
    color: var(--el-color-warning);
  }
}

.mfa-lock-hint {
  margin: 4px 0 0;
  font-size: 12px;
  line-height: 1.5;
  color: var(--el-text-color-secondary);
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
  border: 1px solid var(--el-border-color-lighter);
  border-radius: var(--art-form-control-radius);
}

.setup-fields {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 12px;
  min-width: 0;
}

.inline-field {
  display: flex;
  gap: 8px;
  width: 100%;
}

.backup-block {
  display: flex;
  flex-direction: column;
  gap: 8px;
  width: 100%;
}

.backup-hint {
  margin: 0;
  font-size: 12px;
  line-height: 1.5;
  color: var(--el-text-color-secondary);
}

.backup-codes {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(112px, 1fr));
  gap: 6px 12px;

  code {
    font-family: var(--el-font-family-mono, ui-monospace, monospace);
    font-size: 13px;
    letter-spacing: 0.5px;
    color: var(--el-text-color-primary);
  }
}

.password-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 8px;
}

@media (max-width: 520px) {
  .setup-body {
    flex-direction: column;
  }
}
</style>
