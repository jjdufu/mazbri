<template>
  <ElDialog
    :model-value="view.visible"
    :title="title"
    width="440px"
    append-to-body
    :close-on-click-modal="false"
    :before-close="onBeforeClose"
  >
    <p class="mfa-step-up-dialog__lead">{{ lead }}</p>
    <p v-if="categoryLabel" class="mfa-step-up-dialog__purpose">
      {{ t("common.mfaDialog.categoryLine", { category: categoryLabel }) }}
    </p>
    <ElForm @submit.prevent="onConfirm">
      <ElFormItem :error="errorText">
        <ElInput
          :model-value="code"
          maxlength="8"
          autocomplete="one-time-code"
          :placeholder="t('common.mfaDialog.placeholder')"
          :disabled="view.submitting"
          @update:model-value="onInput"
        />
      </ElFormItem>
      <p class="mfa-step-up-dialog__hint">
        {{ t("common.mfaDialog.backupHint") }}
      </p>
    </ElForm>
    <template #footer>
      <ElButton :disabled="view.submitting" @click="onCancel">
        {{ t("common.cancel") }}
      </ElButton>
      <ElButton type="primary" :loading="view.submitting" @click="onConfirm">
        {{ t("common.confirm") }}
      </ElButton>
    </template>
  </ElDialog>
</template>

<script setup lang="ts">
import { computed, onScopeDispose, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import {
  bindMfaStepUpDialog,
  cancelMfaDialog,
  readMfaDialogView,
  submitMfaDialog,
  type MfaDialogViewModel,
} from "@/utils/http/mfa-dialog-service";

defineOptions({ name: "MfaStepUpDialog" });

const { t, te } = useI18n();
const view = ref<MfaDialogViewModel>(readMfaDialogView());
const code = ref("");
const seenSession = ref(0);

const stop = bindMfaStepUpDialog(() => {
  view.value = readMfaDialogView();
});
onScopeDispose(stop);

watch(
  () => view.value.sessionId,
  (sessionId) => {
    if (sessionId !== seenSession.value) {
      seenSession.value = sessionId;
      code.value = "";
    }
  },
);

const title = computed(() => t("common.mfaDialog.title"));
const lead = computed(() => t("common.mfaDialog.stepUpMessage"));
const categoryLabel = computed(() => {
  const category = view.value.category;
  if (!category) return "";
  const key = `common.mfaDialog.categories.${category}`;
  return te(key) ? t(key) : category;
});
const errorText = computed(() => {
  const errorCode = view.value.errorCode;
  if (!errorCode) return "";
  if (errorCode === "invalid_format") {
    return t("common.mfaDialog.invalidFormat", { digits: view.value.totpDigits });
  }
  if (errorCode === "MFA_CODE_REUSED") return t("common.mfaDialog.reused");
  if (errorCode === "MFA_INVALID_CODE" || errorCode === "MFA_INVALID_REQUEST") {
    if (view.value.attemptsRemaining != null) {
      return t("common.mfaDialog.invalidWithAttempts", {
        n: view.value.attemptsRemaining,
      });
    }
    return t("common.mfaDialog.invalidCode");
  }
  return t("common.mfaDialog.invalidCode");
});

function onInput(value: string): void {
  code.value = value.toUpperCase();
}

function onConfirm(): void {
  submitMfaDialog(code.value);
}

function onCancel(): void {
  cancelMfaDialog(view.value.sessionId);
}

function onBeforeClose(done: () => void): void {
  cancelMfaDialog(view.value.sessionId);
  done();
}
</script>

<style scoped lang="scss">
.mfa-step-up-dialog__lead,
.mfa-step-up-dialog__purpose,
.mfa-step-up-dialog__hint {
  margin: 0 0 12px;
  font-size: 13px;
  line-height: 1.6;
}

.mfa-step-up-dialog__lead {
  color: var(--el-text-color-primary);
}

.mfa-step-up-dialog__purpose {
  color: var(--el-color-warning);
}

.mfa-step-up-dialog__hint {
  color: var(--el-text-color-secondary);
}
</style>
