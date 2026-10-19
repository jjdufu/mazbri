import { ref, reactive, computed, type Ref, type Reactive } from "vue";
import { useRouter } from "vue-router";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import { useUserStore } from "@/store/modules/user";
import { useSettingStore } from "@/store/modules/setting";
import {
  login,
  verifyMfa,
  firstLoginChangePassword,
  mfaRequiredSetup,
  mfaRequiredEnable,
} from "@/api/auth";
import type { LoginResponse, MfaSetupResponse } from "@/types/api";
import { HttpError } from "@/utils/http/error";
import { logger } from "@/utils/sys";
import {
  ErrorCodes,
  STATUS_TO_ERROR_CODE,
  type ErrorCode,
} from "@/constants/error-codes";
import { runWithLoading } from "@/composables/useAsyncState";
export type LoginStep = "login" | "mfa" | "mfaSetup" | "changePassword";
export interface MfaSetupFormData extends MfaSetupResponse {
  totp_code: string;
}
export interface LoginFormData {
  username: string;
  password: string;
  rememberPassword: boolean;
}
export interface MfaFormData {
  totpCode: string;
}
export interface ChangePasswordFormData {
  currentPassword: string;
  newPassword: string;
  confirmPassword: string;
  newUsername: string;
}
const WARN_LOGIN_ERROR_CODES = new Set<ErrorCode>([
  ErrorCodes.AUTH_ERROR,
  ErrorCodes.VALIDATION_ERROR,
  ErrorCodes.PERMISSION_DENIED,
  ErrorCodes.NOT_FOUND,
  ErrorCodes.RATE_LIMITED,
  ErrorCodes.RATE_LIMIT_EXCEEDED,
  ErrorCodes.NETWORK_ERROR,
]);
function resolveLoginErrorCode(status: number): ErrorCode {
  return STATUS_TO_ERROR_CODE[status] ?? ErrorCodes.INTERNAL_ERROR;
}
function resolveLoginSeverity(errorCode: ErrorCode): "warn" | "error" {
  return WARN_LOGIN_ERROR_CODES.has(errorCode) ? "warn" : "error";
}
export function copyToClipboard(text: string): Promise<boolean> {
  return new Promise((resolve) => {
    try {
      if (navigator.clipboard && window.isSecureContext) {
        navigator.clipboard
          .writeText(text)
          .then(() => resolve(true))
          .catch(() => resolve(false));
        return;
      }
      const textarea = document.createElement("textarea");
      textarea.value = text;
      textarea.style.position = "fixed";
      textarea.style.left = "-9999px";
      textarea.style.top = "0";
      document.body.appendChild(textarea);
      textarea.focus();
      textarea.select();
      const success = document.execCommand("copy");
      document.body.removeChild(textarea);
      resolve(success);
    } catch {
      resolve(false);
    }
  });
}
export function useLoginFlow() {
  const { t } = useI18n();
  const router = useRouter();
  const userStore = useUserStore();
  const settingStore = useSettingStore();
  const loginStep: Ref<LoginStep> = ref("login");
  const tempToken: Ref<string> = ref("");
  const userRole: Ref<string> = ref("");
  const totpDigits: Ref<number> = ref(6);
  const loading: Ref<boolean> = ref(false);
  const mfaLoading: Ref<boolean> = ref(false);
  const mfaSetupLoading: Ref<boolean> = ref(false);
  const changePasswordLoading: Ref<boolean> = ref(false);
  const isPassing: Ref<boolean> = ref(false);
  const isClickPass: Ref<boolean> = ref(false);
  const formData: Reactive<LoginFormData> = reactive({
    username: "",
    password: "",
    rememberPassword: true,
  });
  const mfaFormData: Reactive<MfaFormData> = reactive({
    totpCode: "",
  });
  const mfaSetupData: Reactive<MfaSetupFormData> = reactive({
    secret: "",
    qr_code_url: "",
    backup_codes: [],
    totp_code: "",
  });
  const changePasswordFormData: Reactive<ChangePasswordFormData> = reactive({
    currentPassword: "",
    newPassword: "",
    confirmPassword: "",
    newUsername: "",
  });
  function transitionTo(step: LoginStep, token?: string): void {
    loginStep.value = step;
    if (token !== undefined) {
      tempToken.value = token;
    }
  }
  async function handleLoginResponse(
    response: LoginResponse,
  ): Promise<LoginStep | "complete"> {
    if (response.mfa_setup_required && response.temp_token) {
      transitionTo("mfaSetup", response.temp_token);
      await handleMfaSetup();
      return "mfaSetup";
    }
    if (response.mfa_required && response.temp_token) {
      transitionTo("mfa", response.temp_token);
      if (response.totp_digits) {
        totpDigits.value = response.totp_digits;
      }
      return "mfa";
    }
    if (response.password_reset_required) {
      transitionTo("changePassword", response.temp_token || "");
      userRole.value = response.role || "";
      changePasswordFormData.currentPassword = formData.password;
      return "changePassword";
    }
    if (response.user?.force_password_reset) {
      transitionTo(
        "changePassword",
        response.temp_token || response.token || "",
      );
      userRole.value = response.user.role || "";
      changePasswordFormData.currentPassword = formData.password;
      return "changePassword";
    }
    await completeLogin(response);
    return "complete";
  }
  async function handleLogin(credentials?: {
    username: string;
    password: string;
  }): Promise<void> {
    await runWithLoading(loading, async () => {
      const { username, password } = credentials || formData;
      formData.username = username;
      formData.password = password;
      const response = await login({ username, password });
      await handleLoginResponse(response);
    }).catch((error) => {
      handleLoginError(error);
    });
  }
  async function completeLogin(
    response: LoginResponse,
    showLoginMsg = true,
  ): Promise<void> {

    const sessionOk =
      !!response.user &&
      (!!response.token ||
        !!response.cookie_auth ||
        typeof response.expires_in === "number");
    if (!sessionOk || !response.user) {
      ElMessage.error(t("login.error.unexpected"));
      return;
    }
    userStore.setToken(
      response.token || undefined,
      response.expires_in,
      response.refresh_token || undefined,
    );
    userStore.setLoginStatus(true);
    userStore.setUserInfo(response.user);
    try {
      await settingStore.loadFromServer();
    } catch (error) {
      logger.warn(
        "[Login] Failed to load appearance settings; falling back to local config",
        {
          service: "mazbri-mgmt-web",
          event: "login_settings_load_fallback",
          severity: "warn",
          error_code: ErrorCodes.INTERNAL_ERROR,
          error,
        },
      );
    }
    router.push("/");
    if (showLoginMsg) {
      const username = userStore.info.username || "User";
      ElMessage.success(t("login.success.messageWithUser", { username }));
    }
  }
  function handleLoginError(error: unknown): void {
    if (error instanceof HttpError) {
      const errorCode = resolveLoginErrorCode(error.code);
      const severity = resolveLoginSeverity(errorCode);
      if (severity === "warn") {
        logger.warn("[Login] Authentication flow request failed", {
          service: "mazbri-mgmt-web",
          event: "login_http_error",
          severity,
          error_code: errorCode,
          status: error.code,
          url: error.url,
          method: error.method,
          data: error.data,
        });
      } else {
        logger.error("[Login] Authentication flow request failed", {
          service: "mazbri-mgmt-web",
          event: "login_http_error",
          severity,
          error_code: errorCode,
          status: error.code,
          url: error.url,
          method: error.method,
          data: error.data,
        });
      }
    } else {
      logger.error("[Login] Unknown authentication exception", {
        service: "mazbri-mgmt-web",
        event: "login_unexpected_error",
        severity: "error",
        error_code: ErrorCodes.INTERNAL_ERROR,
        error,
      });
      ElMessage.error(t("login.error.unexpected"));
    }
  }
  async function handleMfaVerify(formData?: MfaFormData): Promise<void> {
    await runWithLoading(mfaLoading, async () => {
      const totpCode = formData?.totpCode || mfaFormData.totpCode;
      const response = await verifyMfa({
        temp_token: tempToken.value,
        totp_code: totpCode,
      });
      userStore.setMfaVerified(true);
      await completeLogin(response);
    }).catch((error) => {
      handleLoginError(error);
    });
  }
  async function handleMfaSetup(): Promise<void> {
    await runWithLoading(mfaSetupLoading, async () => {
      const response = await mfaRequiredSetup({ temp_token: tempToken.value });
      Object.assign(mfaSetupData, response);
      if (response.totp_digits) {
        totpDigits.value = response.totp_digits;
      }
    }).catch((error) => {
      handleLoginError(error);
    });
  }
  async function handleMfaSetupEnable(): Promise<void> {
    if (!mfaSetupData.totp_code) {
      ElMessage.warning(t("login.mfa.placeholder"));
      return;
    }
    await runWithLoading(mfaSetupLoading, async () => {
      const response = await mfaRequiredEnable({
        temp_token: tempToken.value,
        totp_code: mfaSetupData.totp_code,
      });
      await completeLogin(response);
    }).catch((error) => {
      handleLoginError(error);
    });
  }
  async function handleChangePassword(): Promise<void> {
    await runWithLoading(changePasswordLoading, async () => {
      const trimmedNewUname = changePasswordFormData.newUsername.trim();
      const response = await firstLoginChangePassword(
        {
          username: formData.username,
          current_password: changePasswordFormData.currentPassword,
          new_password: changePasswordFormData.newPassword,
          new_username:
            trimmedNewUname.length > 0 ? trimmedNewUname : undefined,
        },
        tempToken.value,
      );
      ElMessage.success(t("login.changePassword.success"));
      await completeLogin(response, false);
    }).catch((error) => {
      handleLoginError(error);
    });
  }
  function handleBackToLogin(): void {
    loginStep.value = "login";
    tempToken.value = "";
    mfaFormData.totpCode = "";
  }
  return {
    loginStep,
    tempToken,
    userRole,
    totpDigits,
    loading,
    mfaLoading,
    mfaSetupLoading,
    changePasswordLoading,
    formData,
    mfaFormData,
    mfaSetupData,
    changePasswordFormData,
    isPassing,
    isClickPass,
    transitionTo,
    handleLoginResponse,
    handleLogin,
    handleMfaVerify,
    handleMfaSetup,
    handleMfaSetupEnable,
    handleChangePassword,
    handleBackToLogin,
    completeLogin,
    copyToClipboard,
  };
}
