<template>
  <div class="form mfa-setup-form">
    <div class="mb-4 text-center max-sm:mb-3">
      <h3 class="title !max-sm:text-2xl">{{ t("login.mfaSetup.title") }}</h3>
      <p class="sub-title max-sm:!text-xs">
        {{ t("login.mfaSetup.subTitle") }}
      </p>
    </div>

    <div class="mb-5 flex flex-col items-center max-sm:mb-4">
      <div
        class="mfa-setup-panel p-3 bg-white dark:bg-gray-100 shadow-sm max-sm:p-2"
      >
        <QrcodeVue
          v-if="setupData.qr_code_url"
          :value="setupData.qr_code_url"
          :size="160"
          :level="qrLevel"
          render-as="canvas"
          background="#ffffff"
          foreground="#000000"
          class="max-sm:!w-[140px] max-sm:!h-[140px]"
        />
        <div
          v-else
          class="mfa-setup-placeholder w-[160px] h-[160px] flex items-center justify-center bg-gray-100 max-sm:!w-[140px] max-sm:!h-[140px]"
        >
          <span class="text-gray-400 text-sm max-sm:text-xs">{{
            t("login.mfaSetup.qrPlaceholder")
          }}</span>
        </div>
      </div>
    </div>

    <div v-if="setupData.secret" class="mb-4">
      <p class="text-sm text-gray-600 dark:text-gray-300 mb-2">
        {{ t("login.mfaSetup.secretLabel") }}
      </p>
      <div class="flex items-center gap-2">
        <ElInput
          class="custom-height flex-1"
          :model-value="setupData.secret"
          readonly
          :prefix-icon="Key"
        />
        <ElButton class="custom-height" @click="copySecret">
          {{
            copiedSecret ? t("login.mfaSetup.copied") : t("login.mfaSetup.copy")
          }}
        </ElButton>
      </div>
    </div>

    <div class="mb-4">
      <ElInput
        v-model.trim="setupData.totp_code"
        class="custom-height"
        :placeholder="t('login.mfaSetup.codePlaceholder')"
        :maxlength="totpDigits"
        :prefix-icon="Key"
      />
      <p
        v-if="setupData.secret"
        class="text-xs text-gray-500 dark:text-gray-400 mt-1"
      >
        {{ t("login.mfaSetup.codeHint", { digits: totpDigits }) }}
      </p>
    </div>

    <div
      v-if="setupData.backup_codes && setupData.backup_codes.length > 0"
      class="mb-5 max-sm:mb-4"
    >
      <div
        class="flex items-center justify-between mb-2 max-sm:flex-col max-sm:items-start max-sm:gap-2"
      >
        <p class="text-sm text-gray-600 dark:text-gray-300">
          {{ t("login.mfaSetup.backupCodesLabel") }}
        </p>
        <ElButton size="small" class="max-sm:!w-full" @click="copyBackupCodes">
          {{
            copiedBackup
              ? t("login.mfaSetup.copied")
              : t("login.mfaSetup.copyAll")
          }}
        </ElButton>
      </div>
      <div
        class="mfa-setup-panel p-3 bg-gray-50 dark:bg-gray-800 border border-gray-200 dark:border-gray-600 max-sm:p-2"
      >
        <div class="grid grid-cols-2 gap-2 max-sm:grid-cols-2 max-sm:gap-1.5">
          <div
            v-for="(code, index) in setupData.backup_codes"
            :key="index"
            class="mfa-backup-code text-xs font-mono text-gray-700 dark:text-gray-300 bg-white dark:bg-gray-700 px-2 py-1.5 border border-gray-100 dark:border-gray-600 text-center max-sm:px-1.5 max-sm:py-1"
          >
            {{ code }}
          </div>
        </div>
      </div>
    </div>

    <div class="space-y-3">
      <ElButton
        v-if="!setupData.secret"
        v-ripple
        class="w-full custom-height"
        type="primary"
        :loading="loading"
        @click="handleGetQr"
      >
        {{
          loading
            ? t("login.mfaSetup.gettingQrCode")
            : t("login.mfaSetup.getQrCode")
        }}
      </ElButton>
      <ElButton
        v-else
        v-ripple
        class="w-full custom-height"
        type="primary"
        :loading="loading"
        @click="handleEnable"
      >
        {{ t("login.mfaSetup.btnText") }}
      </ElButton>

      <div class="flex justify-center">
        <ElButton type="text" @click="handleBack">
          {{ t("login.mfaSetup.backToLogin") }}
        </ElButton>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import { Key } from "@element-plus/icons-vue";
import QrcodeVue from "qrcode.vue";
import type { MfaSetupResponse } from "@/types/api";
import { copyToClipboard } from "../composables/useLoginFlow";
interface MfaSetupFormData extends MfaSetupResponse {
  totp_code: string;
}
interface Props {
  loading: boolean;
  setupData: MfaSetupFormData;
  totpDigits: number;
}
interface Emits {
  (e: "get-qr"): void;
  (e: "enable", code: string): void;
  (e: "back"): void;
}
const props = defineProps<Props>();
const emit = defineEmits<Emits>();
const { t } = useI18n();
const qrLevel = ref<"L" | "M" | "Q" | "H">("M");
const copiedSecret = ref(false);
const copiedBackup = ref(false);
const copySecret = async () => {
  if (!props.setupData.secret) return;
  const success = await copyToClipboard(props.setupData.secret);
  if (success) {
    copiedSecret.value = true;
    ElMessage.success(t("login.mfaSetup.secretCopied"));
    setTimeout(() => {
      copiedSecret.value = false;
    }, 2000);
  } else {
    ElMessage.error(t("login.mfaSetup.copyFailed"));
  }
};
const copyBackupCodes = async () => {
  if (
    !props.setupData.backup_codes ||
    props.setupData.backup_codes.length === 0
  )
    return;
  const codesText = props.setupData.backup_codes.join(" ");
  const success = await copyToClipboard(codesText);
  if (success) {
    copiedBackup.value = true;
    ElMessage.success(t("login.mfaSetup.backupCodesCopied"));
    setTimeout(() => {
      copiedBackup.value = false;
    }, 2000);
  } else {
    ElMessage.error(t("login.mfaSetup.copyFailed"));
  }
};
const handleGetQr = () => {
  emit("get-qr");
};
const handleEnable = () => {
  emit("enable", props.setupData.totp_code);
};
const handleBack = () => {
  emit("back");
};
defineExpose({
  qrLevel,
});
</script>

<style scoped lang="scss">
.mfa-setup-panel {
  border-radius: var(--art-popup-radius);
}

.mfa-setup-placeholder {
  border-radius: var(--art-form-control-radius);
}

.mfa-backup-code {
  border-radius: var(--art-form-control-radius-sm);
}
</style>
