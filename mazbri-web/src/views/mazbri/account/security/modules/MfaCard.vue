<template>
  <SettingsCard
    v-loading="statusLoading"
    icon="ri:smartphone-line"
    :title="$t('accountSecurity.mfa.title')"
    :info="mfaInfoText"
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
      <template v-if="mfaEnabled && isAdmin">
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
      <ElButton v-else-if="!mfaEnabled && setupData" @click="cancelSetup">
        {{ $t("common.cancel") }}
      </ElButton>
      <ElButton
        v-else-if="!mfaEnabled"
        type="primary"
        :loading="actionLoading"
        @click="handleStartSetup"
      >
        {{ $t("accountSecurity.actions.bind") }}
      </ElButton>
    </template>

    <template v-if="mfaEnabled">
      <CommonSettingRow
        v-if="isAdmin"
        mode="inline"
        mobile-stack
        :label="$t('securityPolicy.fields.issuerName')"
      >
        <ElInput
          v-model="mfaForm.totp_issuer"
          class="setting-input"
          :disabled="issuerSaving"
          :placeholder="$t('securityPolicy.placeholders.issuerName')"
          @change="handleIssuerSave"
        />
      </CommonSettingRow>
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
    </template>

    <template v-else-if="!setupData">
      <CommonSettingRow
        mode="inline"
        mobile-stack
        :label="$t('accountSecurity.mfa.specLabel')"
      >
        <span class="setting-value">{{
          $t("accountSecurity.mfa.specValue")
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

    <!-- 绑定流程：获取密钥/二维码 → 可实时调整发行者名称 → 输入验证码启用 -->
    <template v-if="!mfaEnabled && setupData">
      <div class="setup-body">
        <div class="qr-wrap">
          <QrcodeVue
            :value="liveQrCodeUrl"
            :size="148"
            level="M"
            render-as="canvas"
            background="#ffffff"
            foreground="#000000"
          />
        </div>
        <div class="setup-fields">
          <CommonSettingRow
            v-if="isAdmin"
            mode="block"
            :label="$t('securityPolicy.fields.issuerName')"
          >
            <ElInput
              v-model="setupIssuer"
              :placeholder="$t('securityPolicy.placeholders.issuerName')"
            />
          </CommonSettingRow>
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
        <div class="backup-codes">
          <code v-for="code in backupCodes" :key="code">{{ code }}</code>
        </div>
        <div>
          <ElButton size="small" @click="copyText(backupCodes.join('\n'))">
            {{ $t("accountSecurity.actions.copyAll") }}
          </ElButton>
        </div>
      </div>
    </CommonSettingRow>
  </SettingsCard>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { ElMessage, ElMessageBox } from "element-plus";
import QrcodeVue from "qrcode.vue";
import SettingsCard from "@/components/common/settings-card/index.vue";
import CommonSettingRow from "@/components/common/setting-row/index.vue";
import {
  disableMfa,
  enableMfa,
  fetchGetUserInfo,
  getMfaSetup,
  getMfaStatus,
  regenerateBackupCodes,
} from "@/api/auth";
import { updateMfaSettings } from "@/api/audit";
import type {
  MfaSessionRefresh,
  MfaSetupResponse,
  MfaStatus,
} from "@/types/api";
import { runWithLoading } from "@/composables/useAsyncState";
import { useOpsRole } from "@/composables/ops/useOpsRole";
import { useUserStore } from "@/store/modules/user";
import { useMfaStepUpStore } from "@/store/modules/mfa-step-up";
import { copyTextToClipboard } from "@/utils/clipboard";
import {
  formatExpiryUtc8,
  readSessionExpiresIn,
} from "@/utils/http/mfa-step-up-state";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-dialog-service";
import { HttpError } from "@/utils/http/error";
import { $t } from "@/locales";
import { logger } from "@/utils/sys";
import { usePlatformSecurityPolicy } from "./usePlatformSecurityPolicy";

defineOptions({ name: "MfaCard" });

const userStore = useUserStore();
const stepUpStore = useMfaStepUpStore();
const { isAdmin } = useOpsRole();
const { mfaForm } = usePlatformSecurityPolicy();
const statusLoading = ref(false);
const actionLoading = ref(false);
const issuerSaving = ref(false);
const status = ref<MfaStatus | null>(null);
const setupData = ref<MfaSetupResponse | null>(null);
const setupCode = ref("");
const setupIssuer = ref("Mazbri");
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
const setupDigits = computed(() => 6);

const mfaInfoText = computed(() => {
  const parts = [$t("accountSecurity.mfa.hint")];
  if (!isAdmin.value) {
    parts.push($t("accountSecurity.mfa.userHint"));
  }
  if (mfaRequired.value) {
    parts.push($t("accountSecurity.mfa.forcedHint"));
  }
  return parts.join("\n");
});

const liveQrCodeUrl = computed(() => {
  if (!setupData.value) return "";
  const issuer = setupIssuer.value.trim() || "Mazbri";
  const username = userStore.info?.username || "admin";
  const secret = setupData.value.secret;
  const encodedIssuer = encodeURIComponent(issuer);
  const encodedAccount = encodeURIComponent(username);
  return `otpauth://totp/${encodedIssuer}:${encodedAccount}?secret=${secret}&issuer=${encodedIssuer}&digits=6&period=30`;
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

async function handleIssuerSave(): Promise<void> {
  if (!isAdmin.value) return;
  const issuer = mfaForm.totp_issuer.trim();
  if (!issuer) {
    ElMessage.warning($t("securityPolicy.messages.issuerRequired"));
    mfaForm.totp_issuer = "Mazbri";
    return;
  }
  await runWithLoading(issuerSaving, async () => {
    await updateMfaSettings({
      totp_issuer: issuer,
      totp_digits: 6,
      totp_period: 30,
      backup_codes_count: 10,
    });
    ElMessage.success($t("securityPolicy.messages.savedSuccess"));
  }).catch((error) => {
    logActionError("Failed to update TOTP issuer:", error);
  });
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
    setupIssuer.value = mfaForm.totp_issuer || "Mazbri";
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
  const issuer = setupIssuer.value.trim() || "Mazbri";
  await runWithLoading(actionLoading, async () => {
    const enabled = await enableMfa({
      totp_code: code,
      totp_issuer: isAdmin.value ? issuer : undefined,
    });
    applyMfaSessionExpiry(enabled);
    if (isAdmin.value) {
      mfaForm.totp_issuer = issuer;
    }
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
  if (!isAdmin.value) return;
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
  if (!isAdmin.value) return;
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

onMounted(() => {
  void loadStatus();
  void stepUpStore.refresh().catch(() => undefined);
});
</script>

<style scoped lang="scss">
.setting-value {
  font-size: 14px;
  color: var(--el-text-color-primary);

  &.is-warning {
    color: var(--el-color-warning);
  }
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

@media (max-width: 640px) {
  .setup-body {
    flex-direction: column;
  }
}
</style>
