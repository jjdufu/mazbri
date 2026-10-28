<template>
  <div class="flex w-full h-screen">
    <LoginLeftView />

    <div class="relative flex-1">
      <AuthTopBar />

      <div class="auth-right-wrap">
        <LoginForm
          v-if="loginStep === 'login'"
          v-model:form-data="formData"
          :loading="loading"
          :is-passing="isPassing"
          :is-click-pass="isClickPass"
          :is-dark="isDark"
          @submit="handleLogin"
          @update:is-passing="isPassing = $event"
          @update:is-click-pass="isClickPass = $event"
        />

        <MfaVerifyForm
          v-else-if="loginStep === 'mfa'"
          v-model:form-data="mfaFormData"
          :loading="mfaLoading"
          :totp-digits="totpDigits"
          @submit="handleMfaVerify"
          @back="handleBackToLogin"
        />

        <MfaSetupForm
          v-else-if="loginStep === 'mfaSetup'"
          v-model:setup-data="mfaSetupData"
          :loading="mfaSetupLoading"
          :totp-digits="totpDigits"
          @get-qr="handleMfaSetup"
          @enable="handleMfaSetupEnable"
          @back="handleBackToLogin"
        />

        <ChangePasswordForm
          v-else-if="loginStep === 'changePassword'"
          v-model:form-data="changePasswordFormData"
          :loading="changePasswordLoading"
          :user-role="userRole"
          @submit="handleChangePassword"
          @back="handleBackToLogin"
        />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useUserStore } from "@/store/modules/user";
import { useSettingStore } from "@/store/modules/setting";
import { useLoginFlow } from "./composables/useLoginFlow";
import LoginForm from "./modules/LoginForm.vue";
import MfaVerifyForm from "./modules/MfaVerifyForm.vue";
import MfaSetupForm from "./modules/MfaSetupForm.vue";
import ChangePasswordForm from "./modules/ChangePasswordForm.vue";
defineOptions({ name: "Login" });
const settingStore = useSettingStore();
const { isDark } = storeToRefs(settingStore);
const {
  loginStep,
  loading,
  formData,
  mfaLoading,
  mfaFormData,
  tempToken,
  userRole,
  totpDigits,
  mfaSetupLoading,
  mfaSetupData,
  changePasswordLoading,
  changePasswordFormData,
  isPassing,
  isClickPass,
  handleLogin: baseHandleLogin,
  handleMfaVerify,
  handleMfaSetup,
  handleMfaSetupEnable,
  handleChangePassword: baseHandlePassword,
  handleBackToLogin,
} = useLoginFlow();
const handleLogin = async (credentials: {
  username: string;
  password: string;
}) => {
  await baseHandleLogin(credentials);
};
const handleChangePassword = async () => {
  await baseHandlePassword();
  // 改密后可能进入 MFA 验证/强制设置；仅在回到登录步时清空表单。
  if (loginStep.value === "login") {
    changePasswordFormData.currentPassword = "";
    changePasswordFormData.newPassword = "";
    changePasswordFormData.confirmPassword = "";
    changePasswordFormData.newUsername = "";
  }
};
</script>

<style scoped>
@import "./style.css";
</style>

<style lang="scss" scoped>
.auth-right-wrap {
  --auth-control-radius: var(--art-form-control-radius);
  --auth-control-bg: var(--el-bg-color);
  --auth-control-bg-active: var(--el-bg-color);
}

:deep(.auth-right-wrap .el-input__wrapper),
:deep(.auth-right-wrap .el-select__wrapper) {
  border-radius: var(--auth-control-radius) !important;
  height: 40px !important;
}

:deep(.auth-right-wrap .el-input__wrapper) {
  border: 1px solid var(--art-form-control-border) !important;
  background-color: var(--auth-control-bg) !important;
  box-shadow: 0 1px 2px 0 rgba(0, 0, 0, 0.05) !important;
  transition:
    border-color var(--el-transition-duration),
    background-color var(--el-transition-duration),
    box-shadow var(--el-transition-duration) !important;
}

:deep(.auth-right-wrap .el-input__wrapper:hover) {
  border-color: var(--art-form-control-border-hover) !important;
}

:deep(.auth-right-wrap .el-input__wrapper.is-focus),
:deep(.auth-right-wrap .el-input__wrapper:focus-within),
:deep(.auth-right-wrap .el-select__wrapper.is-focused) {
  border-color: var(--art-form-control-border-active) !important;
  background-color: var(--auth-control-bg-active) !important;
  box-shadow: 0 0 0 3px rgba(93, 135, 255, 0.15) !important;
}

:deep(.auth-right-wrap .el-input__inner),
:deep(.auth-right-wrap .el-input__prefix),
:deep(.auth-right-wrap .el-input__prefix-inner),
:deep(.auth-right-wrap .el-input__suffix),
:deep(.auth-right-wrap .el-input__suffix-inner),
:deep(.auth-right-wrap .el-select__selection),
:deep(.auth-right-wrap .el-select__selected-item),
:deep(.auth-right-wrap .el-select__placeholder) {
  background: transparent !important;
}

:deep(.auth-right-wrap .el-input__prefix) {
  color: var(--el-text-color-regular) !important;
}

:deep(.auth-right-wrap .el-input__inner:-webkit-autofill),
:deep(.auth-right-wrap .el-input__inner:-webkit-autofill:hover),
:deep(.auth-right-wrap .el-input__inner:-webkit-autofill:focus) {
  -webkit-text-fill-color: var(--el-text-color-primary) !important;
  caret-color: var(--el-text-color-primary);
  box-shadow: 0 0 0 1000px transparent inset !important;
  transition: background-color 999999s ease-out 0s;
}

:deep(.auth-right-wrap .el-button:not(.el-button--text)) {
  border-radius: var(--auth-control-radius) !important;
}

:deep(.auth-right-wrap .el-checkbox__inner) {
  border-radius: var(--art-form-control-radius-sm) !important;
}

:deep(.auth-right-wrap .drag-verify-shell) {
  box-sizing: border-box;
  height: 40px;
  border: 1px solid var(--art-form-control-border) !important;
  border-radius: var(--auth-control-radius) !important;
  box-shadow: 0 1px 2px 0 rgba(0, 0, 0, 0.05) !important;
  overflow: hidden;
}

:deep(.auth-right-wrap .drag-verify-shell.is-error) {
  border-color: #ff4e4f !important;
}


:deep(.auth-right-wrap .drag-verify-shell .drag_verify) {
  width: 100% !important;
  height: 100% !important;
  border: none !important;
  border-radius: inherit !important;
}
</style>
