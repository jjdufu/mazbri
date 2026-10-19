<template>
  <div class="form change-password-form">
    <h3 class="title">{{ t("login.changePassword.title") }}</h3>
    <p class="sub-title">{{ t("login.changePassword.subTitle") }}</p>

    <ElForm
      ref="formRef"
      :model="formData"
      :rules="formRules"
      style="margin-top: 25px"
      @submit.prevent
      @keyup.enter="handleSubmit"
    >
      <ElFormItem prop="currentPassword">
        <ElInput
          v-model.trim="formData.currentPassword"
          class="custom-height"
          :placeholder="t('login.changePassword.currentPassword')"
          type="password"
          autocomplete="off"
          show-password
          :prefix-icon="Lock"
        />
      </ElFormItem>
      <ElFormItem v-if="userRole === 'R_ADMIN'" prop="newUsername">
        <ElInput
          v-model.trim="formData.newUsername"
          class="custom-height"
          :placeholder="t('login.changePassword.newUsername')"
          type="text"
          autocomplete="off"
          :prefix-icon="User"
        />
      </ElFormItem>
      <ElFormItem prop="newPassword">
        <ElInput
          v-model.trim="formData.newPassword"
          class="custom-height"
          :placeholder="t('login.changePassword.newPassword')"
          type="password"
          autocomplete="off"
          show-password
          :prefix-icon="Lock"
        />
      </ElFormItem>
      <ElFormItem prop="confirmPassword">
        <ElInput
          v-model.trim="formData.confirmPassword"
          class="custom-height"
          :placeholder="t('login.changePassword.confirmPassword')"
          type="password"
          autocomplete="off"
          show-password
          :prefix-icon="Lock"
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
          {{ t("login.changePassword.btnText") }}
        </ElButton>
      </div>
    </ElForm>
  </div>
</template>

<script setup lang="ts">
import { ref, reactive } from "vue";
import { useI18n } from "vue-i18n";
import type { FormInstance, FormRules } from "element-plus";
import { Lock, User } from "@element-plus/icons-vue";
import type { ChangePasswordFormData } from "../types";
interface Props {
  loading: boolean;
  formData: ChangePasswordFormData;
  userRole?: string;
}
interface Emits {
  (e: "submit"): void;
}
const props = defineProps<Props>();
const emit = defineEmits<Emits>();
const { t } = useI18n();
const formRef = ref<FormInstance>();
const formRules = reactive<FormRules>({
  currentPassword: [
    {
      required: true,
      message: t("login.changePassword.currentPasswordRequired"),
      trigger: "blur",
    },
  ],
  newUsername: [
    {
      validator: (
        _rule: unknown,
        value: string,
        callback: (error?: Error) => void,
      ) => {
        if (value && (value.trim().length < 3 || value.trim().length > 50)) {
          callback(new Error(t("login.changePassword.usernameLengthError")));
        } else {
          callback();
        }
      },
      trigger: "blur",
    },
  ],
  newPassword: [
    {
      required: true,
      message: t("login.changePassword.newPasswordRequired"),
      trigger: "blur",
    },
    {
      validator: (
        _rule: unknown,
        value: string,
        callback: (error?: Error) => void,
      ) => {
        if (!value) {
          callback();
          return;
        }
        if (value.length < 8) {
          callback(new Error(t("login.changePassword.passwordMinLength")));
          return;
        }
        const hasUpper = /[A-Z]/.test(value);
        const hasLower = /[a-z]/.test(value);
        const hasDigit = /[0-9]/.test(value);
        if (!hasUpper || !hasLower || !hasDigit) {
          callback(new Error("密码必须包含大小写字母和数字"));
          return;
        }
        callback();
      },
      trigger: ["blur", "change"],
    },
  ],
  confirmPassword: [
    {
      required: true,
      message: t("login.changePassword.confirmPasswordRequired"),
      trigger: "blur",
    },
    {
      validator: (
        _rule: unknown,
        value: string,
        callback: (error?: Error) => void,
      ) => {
        if (value !== props.formData.newPassword) {
          callback(new Error(t("login.changePassword.passwordMismatch")));
        } else {
          callback();
        }
      },
      trigger: "blur",
    },
  ],
});
async function handleSubmit() {
  if (!formRef.value) return;
  await formRef.value.validate((valid) => {
    if (valid) {
      emit("submit");
    }
  });
}
</script>

<style scoped>
.change-password-form {
  padding: 20px;
}

.title {
  font-size: 1.25rem;
  font-weight: 600;
  margin-bottom: 0.5rem;
  text-align: center;
}

.sub-title {
  font-size: 0.875rem;
  color: #6b7280;
  margin-bottom: 1.5rem;
  text-align: center;
}

.custom-height {
  height: 40px;
}
</style>
