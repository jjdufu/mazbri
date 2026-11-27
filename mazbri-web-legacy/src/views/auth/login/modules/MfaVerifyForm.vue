<template>
  <div class="form mfa-form">
    <h3 class="title !max-sm:text-2xl">{{ t("login.mfa.title") }}</h3>
    <p class="sub-title">{{ t("login.mfa.subTitle") }}</p>
    <ElForm
      ref="mfaFormRef"
      :model="formModel"
      :rules="mfaRules"
      style="margin-top: 25px"
      @submit.prevent
      @keyup.enter="handleSubmit"
    >
      <ElFormItem prop="totpCode">
        <ElInput
          :model-value="formModel.totpCode"
          class="custom-height"
          :placeholder="t('login.mfa.placeholder')"
          maxlength="8"
          :prefix-icon="Key"
          autocomplete="one-time-code"
          @update:model-value="onCodeInput"
        />
      </ElFormItem>

      <div style="margin-top: 30px">
        <ElButton
          v-ripple
          class="w-full custom-height"
          type="primary"
          :loading="loading"
          @click="handleSubmit"
        >
          {{ t("login.mfa.btnText") }}
        </ElButton>
      </div>

      <div class="mt-4 text-center">
        <ElButton type="text" @click="handleBack">
          {{ t("login.mfa.backToLogin") }}
        </ElButton>
      </div>
    </ElForm>
  </div>
</template>

<script setup lang="ts">
import { ref, reactive, computed, watch } from "vue";
import { useI18n } from "vue-i18n";
import type { FormInstance, FormRules } from "element-plus";
import { Key } from "@element-plus/icons-vue";
import { classifySubmittedCode } from "@/utils/http/mfa-step-up-state";
interface FormData {
  totpCode: string;
}
interface Props {
  loading: boolean;
  totpDigits: number;
  formData: FormData;
}
interface Emits {
  (e: "update:form-data", value: FormData): void;
  (e: "submit", value: FormData): void;
  (e: "back"): void;
}
const props = defineProps<Props>();
const emit = defineEmits<Emits>();
const { t } = useI18n();
const mfaFormRef = ref<FormInstance>();
const formModel = reactive<FormData>({
  totpCode: props.formData.totpCode || "",
});
watch(
  () => props.formData,
  (newPropData) => {
    if (newPropData && newPropData.totpCode !== formModel.totpCode) {
      formModel.totpCode = newPropData.totpCode || "";
    }
  },
  { deep: true },
);
function onCodeInput(value: string): void {
  formModel.totpCode = value.trim().toUpperCase();
}
const mfaRules = computed<FormRules>(() => ({
  totpCode: [
    { required: true, message: t("login.mfa.placeholder"), trigger: "blur" },
    {
      validator: (_rule, value: string, callback) => {
        if (classifySubmittedCode(value ?? "", props.totpDigits)) {
          callback();
          return;
        }
        callback(new Error(t("login.mfa.invalidCode")));
      },
      trigger: "blur",
    },
  ],
}));
const handleSubmit = async () => {
  if (!mfaFormRef.value) return;
  try {
    const valid = await mfaFormRef.value.validate();
    if (!valid) return;
    emit("submit", { ...formModel });
  } catch {
    return;
  }
};
const handleBack = () => {
  emit("back");
};
defineExpose({
  mfaFormRef,
});
</script>
