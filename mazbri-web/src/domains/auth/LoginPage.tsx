import { type FormEvent, useCallback, useState } from "react";
import { useNavigate, useSearchParams } from "react-router";
import {
  ArrowLeft,
  Check,
  Copy,
  KeyRound,
  Moon,
  Shield,
  Sun,
} from "lucide-react";
import { QRCodeSVG } from "qrcode.react";
import {
  Button,
  Card,
  FormField,
  IconButton,
  Input,
  PasswordInput,
  Tooltip,
  cn,
} from "@/components/ui";
import { resolveDefaultHomePath } from "@/components/layout/menuConfig";
import {
  firstLoginChangePassword,
  login,
  mfaRequiredEnable,
  mfaRequiredSetup,
  verifyMfa,
} from "@/api/auth";
import {
  ErrorCodes,
  STATUS_TO_ERROR_CODE,
  type ErrorCode,
} from "@/constants/error-codes";
import { LanguageEnum } from "@/enums/appEnum";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import { useMfaStepUpStore } from "@/stores/mfa-step-up";
import { useThemeStore } from "@/stores/theme";
import { useWatermarkStore } from "@/stores/watermark";
import type { LoginResponse, MfaSetupResponse } from "@/types/api";
import { HttpError } from "@/utils/http/error";
import { classifySubmittedCode } from "@/utils/http/mfa-step-up-state";
import { ApiStatus } from "@/utils/http/status";
import { logger } from "@/utils/sys/logger";
import { copyWithFeedback } from "@/utils/ui/copy-feedback";
import {
  showErrorToast,
  showSuccessToast,
  showWarningToast,
} from "@/utils/ui/message-guard";

type LoginStep = "login" | "mfa" | "mfaSetup" | "changePassword";

const LOGIN_SLIDER_ENABLED =
  import.meta.env.VITE_LOGIN_SLIDER_ENABLED === "true";

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

function resolveSafeRedirect(
  rawRedirect: string | null,
  fallbackPath: string,
): string {
  if (!rawRedirect) return fallbackPath;
  const trimmed = rawRedirect.trim();
  if (
    !trimmed.startsWith("/") ||
    trimmed.startsWith("//") ||
    trimmed.startsWith("/auth/login") ||
    /[\r\n]/.test(trimmed)
  ) {
    return fallbackPath;
  }
  return trimmed;
}

export function LoginPage() {
  const { t, locale, toggleLocale } = useT();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();

  const setToken = useAuthStore((s) => s.setToken);
  const setLoginStatus = useAuthStore((s) => s.setLoginStatus);
  const setUserInfo = useAuthStore((s) => s.setUserInfo);
  const isDark = useThemeStore((s) => s.isDark);
  const toggleTheme = useThemeStore((s) => s.toggleTheme);

  const [step, setStep] = useState<LoginStep>("login");
  const [tempToken, setTempToken] = useState("");
  const [userRole, setUserRole] = useState("");
  const [totpDigits, setTotpDigits] = useState<6 | 8>(6);

  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [loginError, setLoginError] = useState<{
    username?: string;
    password?: string;
    slider?: string;
  }>({});
  const [sliderValue, setSliderValue] = useState(0);
  const [sliderPassed, setSliderPassed] = useState(false);

  const [mfaCode, setMfaCode] = useState("");
  const [mfaError, setMfaError] = useState<string | undefined>(undefined);

  const [mfaSetupData, setMfaSetupData] = useState<MfaSetupResponse>({
    secret: "",
    qr_code_url: "",
    backup_codes: [],
  });
  const [mfaSetupCode, setMfaSetupCode] = useState("");
  const [copiedSecret, setCopiedSecret] = useState(false);
  const [copiedBackup, setCopiedBackup] = useState(false);

  const [changeForm, setChangeForm] = useState({
    currentPassword: "",
    newUsername: "",
    newPassword: "",
    confirmPassword: "",
  });
  const [changeErrors, setChangeErrors] = useState<{
    currentPassword?: string;
    newUsername?: string;
    newPassword?: string;
    confirmPassword?: string;
  }>({});

  const [loading, setLoading] = useState(false);
  const [mfaLoading, setMfaLoading] = useState(false);
  const [mfaSetupLoading, setMfaSetupLoading] = useState(false);
  const [changePasswordLoading, setChangePasswordLoading] = useState(false);

  const handleBackToLogin = useCallback(() => {
    setStep("login");
    setTempToken("");
    setUserRole("");
    setTotpDigits(6);
    setMfaCode("");
    setMfaError(undefined);
    setMfaSetupData({ secret: "", qr_code_url: "", backup_codes: [] });
    setMfaSetupCode("");
    setChangeForm({
      currentPassword: "",
      newUsername: "",
      newPassword: "",
      confirmPassword: "",
    });
    setChangeErrors({});
    setSliderValue(0);
    setSliderPassed(false);
  }, []);

  const handleLoginError = useCallback(
    (error: unknown) => {
      if (error instanceof HttpError) {
        const errorCode = resolveLoginErrorCode(error.code);
        const severity = WARN_LOGIN_ERROR_CODES.has(errorCode)
          ? "warn"
          : "error";
        const payload = {
          service: "mazbri-mgmt-web",
          event: "login_http_error",
          severity,
          error_code: errorCode,
          status: error.code,
          url: error.url,
          method: error.method,
          data: error.data,
        };
        if (severity === "warn") {
          logger.warn("[Login] Authentication flow request failed", payload);
        } else {
          logger.error("[Login] Authentication flow request failed", payload);
        }
      } else {
        logger.error("[Login] Unknown authentication exception", {
          service: "mazbri-mgmt-web",
          event: "login_unexpected_error",
          severity: "error",
          error_code: ErrorCodes.INTERNAL_ERROR,
          error,
        });
        showErrorToast(t("login.error.unexpected"));
      }
    },
    [t],
  );

  const handleStageError = useCallback(
    (error: unknown, expiredMessage: string) => {
      handleLoginError(error);
      if (error instanceof HttpError && error.code === ApiStatus.unauthorized) {
        handleBackToLogin();
        showWarningToast(expiredMessage);
      }
    },
    [handleBackToLogin, handleLoginError],
  );

  const completeLogin = useCallback(
    async (response: LoginResponse, showLoginMsg = true) => {
      const sessionOk =
        Boolean(response.user) &&
        (Boolean(response.token) ||
          Boolean(response.cookie_auth) ||
          typeof response.expires_in === "number");
      if (!sessionOk || !response.user) {
        showErrorToast(t("login.error.unexpected"));
        return;
      }

      setToken(
        response.token || undefined,
        response.expires_in,
        response.refresh_token || undefined,
      );
      setLoginStatus(true);
      setUserInfo(response.user);

      void useMfaStepUpStore
        .getState()
        .refresh(true)
        .catch(() => undefined);
      void useWatermarkStore
        .getState()
        .loadFromServer(true)
        .catch(() => undefined);

      if (showLoginMsg) {
        showSuccessToast(
          t("login.success.messageWithUser", {
            username: response.user.username || "User",
          }),
        );
      }

      const fallbackPath = resolveDefaultHomePath(response.user.role);
      const target = resolveSafeRedirect(
        searchParams.get("redirect"),
        fallbackPath,
      );
      navigate(target, { replace: true });
    },
    [navigate, searchParams, setLoginStatus, setToken, setUserInfo, t],
  );

  const fetchMfaSetupData = useCallback(
    async (tokenToUse: string) => {
      setMfaSetupLoading(true);
      try {
        const { temp_token: nextToken, ...setup } = await mfaRequiredSetup({
          temp_token: tokenToUse,
        });
        if (nextToken) {
          setTempToken(nextToken);
        }
        setMfaSetupData(setup);
        if (setup.totp_digits) {
          setTotpDigits(setup.totp_digits === 8 ? 8 : 6);
        }
      } catch (error) {
        handleStageError(error, t("login.mfa.sessionExpired"));
      } finally {
        setMfaSetupLoading(false);
      }
    },
    [handleStageError, t],
  );

  const handleLoginResponse = useCallback(
    async (
      response: LoginResponse,
      currentPasswordSnapshot: string,
      showLoginMsg = true,
    ) => {
      if (response.mfa_setup_required && response.temp_token) {
        setTempToken(response.temp_token);
        setStep("mfaSetup");
        await fetchMfaSetupData(response.temp_token);
        return;
      }
      if (response.mfa_required && response.temp_token) {
        setTempToken(response.temp_token);
        if (response.totp_digits) {
          setTotpDigits(response.totp_digits === 8 ? 8 : 6);
        }
        setMfaCode("");
        setMfaError(undefined);
        setStep("mfa");
        return;
      }
      if (response.password_reset_required || response.user?.force_password_reset) {
        setTempToken(response.temp_token || "");
        setUserRole(response.role || response.user?.role || "");
        setChangeForm({
          currentPassword: currentPasswordSnapshot,
          newUsername: "",
          newPassword: "",
          confirmPassword: "",
        });
        setChangeErrors({});
        setStep("changePassword");
        return;
      }
      await completeLogin(response, showLoginMsg);
    },
    [completeLogin, fetchMfaSetupData],
  );

  const handleLoginSubmit = async (event: FormEvent) => {
    event.preventDefault();
    const trimmedUser = username.trim();
    const nextErrors: { username?: string; password?: string; slider?: string } =
      {};
    if (!trimmedUser) {
      nextErrors.username = t("login.placeholder.username");
    }
    if (!password) {
      nextErrors.password = t("login.placeholder.password");
    }
    if (LOGIN_SLIDER_ENABLED && !sliderPassed) {
      nextErrors.slider = t("login.placeholder.slider");
    }
    if (Object.keys(nextErrors).length > 0) {
      setLoginError(nextErrors);
      return;
    }

    setLoginError({});
    setLoading(true);
    try {
      const response = await login({ username: trimmedUser, password });
      await handleLoginResponse(response, password, true);
    } catch (error) {
      handleLoginError(error);
    } finally {
      setLoading(false);
    }
  };

  const handleChangePasswordSubmit = async (event: FormEvent) => {
    event.preventDefault();
    const nextErrors: {
      currentPassword?: string;
      newUsername?: string;
      newPassword?: string;
      confirmPassword?: string;
    } = {};

    if (!changeForm.currentPassword.trim()) {
      nextErrors.currentPassword = t(
        "login.changePassword.currentPasswordRequired",
      );
    }
    const trimmedNewUsername = changeForm.newUsername.trim();
    if (
      userRole === "R_ADMIN" &&
      trimmedNewUsername &&
      (trimmedNewUsername.length < 3 || trimmedNewUsername.length > 50)
    ) {
      nextErrors.newUsername = t("login.changePassword.usernameLengthError");
    }
    if (!changeForm.newPassword) {
      nextErrors.newPassword = t("login.changePassword.newPasswordRequired");
    } else if (changeForm.newPassword.length < 8) {
      nextErrors.newPassword = t("login.changePassword.passwordMinLength");
    } else if (
      !/[A-Z]/.test(changeForm.newPassword) ||
      !/[a-z]/.test(changeForm.newPassword) ||
      !/[0-9]/.test(changeForm.newPassword)
    ) {
      nextErrors.newPassword = t("login.changePassword.passwordComplexity");
    }
    if (!changeForm.confirmPassword) {
      nextErrors.confirmPassword = t(
        "login.changePassword.confirmPasswordRequired",
      );
    } else if (changeForm.confirmPassword !== changeForm.newPassword) {
      nextErrors.confirmPassword = t("login.changePassword.passwordMismatch");
    }

    if (Object.keys(nextErrors).length > 0) {
      setChangeErrors(nextErrors);
      return;
    }

    setChangeErrors({});
    setChangePasswordLoading(true);
    try {
      const response = await firstLoginChangePassword(
        {
          username: username.trim(),
          current_password: changeForm.currentPassword,
          new_password: changeForm.newPassword,
          new_username:
            userRole === "R_ADMIN" && trimmedNewUsername
              ? trimmedNewUsername
              : undefined,
        },
        tempToken,
      );
      showSuccessToast(t("login.changePassword.success"));
      await handleLoginResponse(response, changeForm.newPassword, false);
    } catch (error) {
      handleStageError(error, t("login.changePassword.sessionExpired"));
    } finally {
      setChangePasswordLoading(false);
    }
  };

  const handleMfaVerifySubmit = async (event: FormEvent) => {
    event.preventDefault();
    const parsed = classifySubmittedCode(mfaCode, totpDigits);
    if (!parsed) {
      setMfaError(t("login.mfa.invalidCode"));
      return;
    }
    setMfaError(undefined);
    setMfaLoading(true);
    try {
      const response = await verifyMfa({
        temp_token: tempToken,
        ...parsed,
      });
      await completeLogin(response, true);
    } catch (error) {
      handleStageError(error, t("login.mfa.sessionExpired"));
    } finally {
      setMfaLoading(false);
    }
  };

  const handleMfaSetupEnable = async (event: FormEvent) => {
    event.preventDefault();
    const trimmed = mfaSetupCode.trim();
    if (!trimmed) {
      showWarningToast(t("login.mfa.placeholder"));
      return;
    }
    setMfaSetupLoading(true);
    try {
      const response = await mfaRequiredEnable({
        temp_token: tempToken,
        totp_code: trimmed,
      });
      await completeLogin(response, true);
    } catch (error) {
      handleStageError(error, t("login.mfa.sessionExpired"));
    } finally {
      setMfaSetupLoading(false);
    }
  };

  const handleCopySecret = async () => {
    if (!mfaSetupData.secret) return;
    if (await copyWithFeedback(mfaSetupData.secret)) {
      setCopiedSecret(true);
      setTimeout(() => setCopiedSecret(false), 1200);
    }
  };

  const handleCopyBackupCodes = async () => {
    if (!mfaSetupData.backup_codes?.length) return;
    if (await copyWithFeedback(mfaSetupData.backup_codes.join(" "))) {
      setCopiedBackup(true);
      setTimeout(() => setCopiedBackup(false), 1200);
    }
  };

  const themeTooltip = isDark
    ? locale === LanguageEnum.ZH
      ? "浅色模式"
      : "Light mode"
    : locale === LanguageEnum.ZH
      ? "暗色模式"
      : "Dark mode";

  const localeTooltip =
    locale === LanguageEnum.ZH ? "Switch to English" : "切换为中文";

  return (
    <div className="min-h-screen w-full flex bg-canvas-bg text-foreground">
      <section className="hidden lg:flex lg:w-[44%] xl:w-[46%] bg-sidebar-bg text-slate-200 flex-col justify-between p-10 relative overflow-hidden shrink-0">
        <div className="flex items-center gap-2.5">
          <span className="inline-flex h-8 w-8 items-center justify-center rounded-md bg-sidebar-active text-white">
            <Shield className="h-4 w-4" />
          </span>
          <span className="text-base font-semibold tracking-tight text-white">
            Mazbri
          </span>
        </div>

        <div className="space-y-4 max-w-md">
          <h1 className="text-2xl font-semibold tracking-tight text-white leading-snug">
            {t("login.leftView.title")}
          </h1>
          <p className="text-sm text-slate-300 leading-relaxed">
            {t("login.leftView.subTitle")}
          </p>
        </div>

        <div className="text-xs text-slate-400 font-mono-tech">
          Mazbri Gateway Console
        </div>
      </section>

      <section className="flex-1 flex flex-col bg-canvas-bg min-w-0">
        <div className="flex items-center justify-between lg:justify-end gap-2 px-6 py-4 shrink-0">
          <div className="flex items-center gap-2 lg:hidden">
            <span className="inline-flex h-7 w-7 items-center justify-center rounded-md bg-sidebar-active text-white">
              <Shield className="h-4 w-4" />
            </span>
            <span className="text-sm font-semibold text-foreground">
              Mazbri
            </span>
          </div>

          <div className="flex items-center gap-1.5">
            <Tooltip content={localeTooltip} side="bottom">
              <Button
                type="button"
                variant="ghost"
                size="md"
                onClick={toggleLocale}
                aria-label={localeTooltip}
                data-testid="login-locale-btn"
                className="px-2 font-mono-tech text-xs"
              >
                {locale === LanguageEnum.ZH ? "中/EN" : "EN/中"}
              </Button>
            </Tooltip>

            <Tooltip content={themeTooltip} side="bottom">
              <IconButton
                size="md"
                onClick={toggleTheme}
                aria-label={themeTooltip}
                data-testid="login-theme-btn"
              >
                {isDark ? (
                  <Sun className="h-4 w-4" />
                ) : (
                  <Moon className="h-4 w-4" />
                )}
              </IconButton>
            </Tooltip>
          </div>
        </div>

        <div className="flex-1 flex items-center justify-center px-4 pb-10">
          <Card className="w-full max-w-md p-6 sm:p-8 shadow-sm">
            {step === "login" && (
              <form
                onSubmit={(e) => void handleLoginSubmit(e)}
                className="space-y-4"
              >
                <div className="space-y-1">
                  <h2 className="text-lg font-semibold text-foreground">
                    {t("login.title")}
                  </h2>
                  <p className="text-xs text-muted-foreground">
                    {t("login.subTitle")}
                  </p>
                </div>

                <FormField
                  label={locale === LanguageEnum.ZH ? "账号" : "Username"}
                  htmlFor="login-username-input"
                  error={loginError.username}
                  required
                >
                  <Input
                    id="login-username-input"
                    name="username"
                    value={username}
                    autoFocus
                    autoComplete="username"
                    placeholder={t("login.placeholder.username")}
                    data-testid="login-username"
                    onChange={(e) => {
                      setUsername(e.target.value);
                      if (loginError.username) {
                        setLoginError((prev) => ({
                          ...prev,
                          username: undefined,
                        }));
                      }
                    }}
                  />
                </FormField>

                <FormField
                  label={locale === LanguageEnum.ZH ? "密码" : "Password"}
                  htmlFor="login-password-input"
                  error={loginError.password}
                  required
                >
                  <PasswordInput
                    id="login-password-input"
                    name="password"
                    value={password}
                    autoComplete="current-password"
                    placeholder={t("login.placeholder.password")}
                    data-testid="login-password"
                    onChange={(e) => {
                      setPassword(e.target.value);
                      if (loginError.password) {
                        setLoginError((prev) => ({
                          ...prev,
                          password: undefined,
                        }));
                      }
                    }}
                  />
                </FormField>

                {LOGIN_SLIDER_ENABLED && (
                  <div className="space-y-1.5">
                    <div
                      className={cn(
                        "rounded-md border px-3 py-2 space-y-1.5",
                        sliderPassed
                          ? "border-emerald-500/40 bg-emerald-500/10"
                          : "border-border bg-muted/50",
                      )}
                    >
                      <div className="flex items-center justify-between text-xs">
                        <span
                          className={cn(
                            sliderPassed
                              ? "text-emerald-700 dark:text-emerald-400 font-medium"
                              : "text-muted-foreground",
                          )}
                        >
                          {sliderPassed
                            ? t("login.sliderSuccessText")
                            : t("login.sliderText")}
                        </span>
                        {sliderPassed && (
                          <Check className="h-3.5 w-3.5 text-emerald-600 dark:text-emerald-400" />
                        )}
                      </div>
                      <input
                        type="range"
                        min={0}
                        max={100}
                        value={sliderPassed ? 100 : sliderValue}
                        disabled={sliderPassed}
                        data-testid="login-slider"
                        onChange={(e) => {
                          const next = Number(e.target.value);
                          setSliderValue(next);
                          if (next >= 100) {
                            setSliderPassed(true);
                            setLoginError((prev) => ({
                              ...prev,
                              slider: undefined,
                            }));
                          }
                        }}
                        onMouseUp={() => {
                          if (!sliderPassed) setSliderValue(0);
                        }}
                        onTouchEnd={() => {
                          if (!sliderPassed) setSliderValue(0);
                        }}
                        className="w-full cursor-pointer accent-xblue"
                      />
                    </div>
                    {loginError.slider && (
                      <p className="text-[11px] text-red-600 dark:text-red-400">
                        {loginError.slider}
                      </p>
                    )}
                  </div>
                )}

                <Button
                  type="submit"
                  variant="primary"
                  loading={loading}
                  data-testid="login-submit"
                  className="w-full"
                >
                  {t("login.btnText")}
                </Button>
              </form>
            )}

            {step === "changePassword" && (
              <form
                onSubmit={(e) => void handleChangePasswordSubmit(e)}
                className="space-y-4"
              >
                <div className="space-y-1">
                  <h2 className="text-lg font-semibold text-foreground">
                    {t("login.changePassword.title")}
                  </h2>
                  <p className="text-xs text-muted-foreground">
                    {t("login.changePassword.subTitle")}
                  </p>
                </div>

                <FormField
                  label={t("accountSecurity.password.current")}
                  error={changeErrors.currentPassword}
                  required
                >
                  <PasswordInput
                    value={changeForm.currentPassword}
                    autoComplete="current-password"
                    placeholder={t("login.changePassword.currentPassword")}
                    data-testid="change-password-current"
                    onChange={(e) =>
                      setChangeForm((prev) => ({
                        ...prev,
                        currentPassword: e.target.value,
                      }))
                    }
                  />
                </FormField>

                {userRole === "R_ADMIN" && (
                  <FormField
                    label={t("login.changePassword.newUsername")}
                    error={changeErrors.newUsername}
                  >
                    <Input
                      value={changeForm.newUsername}
                      autoComplete="off"
                      placeholder={t("login.changePassword.newUsername")}
                      data-testid="change-password-username"
                      onChange={(e) =>
                        setChangeForm((prev) => ({
                          ...prev,
                          newUsername: e.target.value,
                        }))
                      }
                    />
                  </FormField>
                )}

                <FormField
                  label={t("accountSecurity.password.new")}
                  error={changeErrors.newPassword}
                  required
                >
                  <PasswordInput
                    value={changeForm.newPassword}
                    autoComplete="new-password"
                    placeholder={t("login.changePassword.newPassword")}
                    data-testid="change-password-new"
                    onChange={(e) =>
                      setChangeForm((prev) => ({
                        ...prev,
                        newPassword: e.target.value,
                      }))
                    }
                  />
                </FormField>

                <FormField
                  label={t("accountSecurity.password.confirm")}
                  error={changeErrors.confirmPassword}
                  required
                >
                  <PasswordInput
                    value={changeForm.confirmPassword}
                    autoComplete="new-password"
                    placeholder={t("login.changePassword.confirmPassword")}
                    data-testid="change-password-confirm"
                    onChange={(e) =>
                      setChangeForm((prev) => ({
                        ...prev,
                        confirmPassword: e.target.value,
                      }))
                    }
                  />
                </FormField>

                <div className="space-y-2 pt-1">
                  <Button
                    type="submit"
                    variant="primary"
                    loading={changePasswordLoading}
                    data-testid="change-password-submit"
                    className="w-full"
                  >
                    {t("login.changePassword.btnText")}
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={handleBackToLogin}
                    className="w-full"
                  >
                    <ArrowLeft className="h-3.5 w-3.5" />
                    {t("login.changePassword.backToLogin")}
                  </Button>
                </div>
              </form>
            )}

            {step === "mfaSetup" && (
              <form
                onSubmit={(e) => void handleMfaSetupEnable(e)}
                className="space-y-4"
              >
                <div className="space-y-1 text-center">
                  <h2 className="text-lg font-semibold text-foreground">
                    {t("login.mfaSetup.title")}
                  </h2>
                  <p className="text-xs text-muted-foreground">
                    {t("login.mfaSetup.subTitle")}
                  </p>
                </div>

                <div className="flex justify-center">
                  {mfaSetupData.qr_code_url ? (
                    <div className="qr-surface inline-flex rounded-md p-2.5 border border-border">
                      <QRCodeSVG
                        value={mfaSetupData.qr_code_url}
                        size={160}
                        level="M"
                        bgColor="#ffffff"
                        fgColor="#000000"
                      />
                    </div>
                  ) : (
                    <div className="h-[180px] w-[180px] rounded-md border border-dashed border-border bg-muted flex items-center justify-center p-4 text-center text-xs text-muted-foreground">
                      {t("login.mfaSetup.qrPlaceholder")}
                    </div>
                  )}
                </div>

                {mfaSetupData.secret && (
                  <FormField label={t("login.mfaSetup.secretLabel")}>
                    <div className="flex items-center gap-2">
                      <Input
                        value={mfaSetupData.secret}
                        readOnly
                        className="font-mono-tech"
                      />
                      <Button
                        type="button"
                        variant="outline"
                        onClick={() => void handleCopySecret()}
                      >
                        <Copy className="h-3.5 w-3.5" />
                        {copiedSecret
                          ? t("login.mfaSetup.copied")
                          : t("login.mfaSetup.copy")}
                      </Button>
                    </div>
                  </FormField>
                )}

                <FormField
                  label={t("login.mfaSetup.codePlaceholder")}
                  hint={
                    mfaSetupData.secret
                      ? t("login.mfaSetup.codeHint", { digits: totpDigits })
                      : undefined
                  }
                  required
                >
                  <Input
                    value={mfaSetupCode}
                    maxLength={totpDigits}
                    inputMode="numeric"
                    autoComplete="one-time-code"
                    placeholder={t("login.mfaSetup.codePlaceholder")}
                    data-testid="mfa-setup-code-input"
                    onChange={(e) => setMfaSetupCode(e.target.value.trim())}
                  />
                </FormField>

                {mfaSetupData.backup_codes.length > 0 && (
                  <div className="space-y-2">
                    <div className="flex items-center justify-between gap-2">
                      <span className="text-xs font-medium text-foreground">
                        {t("login.mfaSetup.backupCodesLabel")}
                      </span>
                      <Button
                        type="button"
                        variant="outline"
                        size="xs"
                        onClick={() => void handleCopyBackupCodes()}
                      >
                        {copiedBackup
                          ? t("login.mfaSetup.copied")
                          : t("login.mfaSetup.copyAll")}
                      </Button>
                    </div>
                    <div className="grid grid-cols-2 gap-1.5 rounded-md border border-border bg-muted/50 p-2.5">
                      {mfaSetupData.backup_codes.map((code) => (
                        <code
                          key={code}
                          className="rounded border border-border bg-card px-2 py-1 text-center font-mono-tech text-xs text-foreground"
                        >
                          {code}
                        </code>
                      ))}
                    </div>
                  </div>
                )}

                <div className="space-y-2 pt-1">
                  {!mfaSetupData.secret ? (
                    <Button
                      type="button"
                      variant="primary"
                      loading={mfaSetupLoading}
                      onClick={() => void fetchMfaSetupData(tempToken)}
                      className="w-full"
                    >
                      {mfaSetupLoading
                        ? t("login.mfaSetup.gettingQrCode")
                        : t("login.mfaSetup.getQrCode")}
                    </Button>
                  ) : (
                    <Button
                      type="submit"
                      variant="primary"
                      loading={mfaSetupLoading}
                      data-testid="mfa-setup-enable"
                      className="w-full"
                    >
                      {t("login.mfaSetup.btnText")}
                    </Button>
                  )}
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={handleBackToLogin}
                    className="w-full"
                  >
                    <ArrowLeft className="h-3.5 w-3.5" />
                    {t("login.mfaSetup.backToLogin")}
                  </Button>
                </div>
              </form>
            )}

            {step === "mfa" && (
              <form
                onSubmit={(e) => void handleMfaVerifySubmit(e)}
                className="space-y-4"
              >
                <div className="space-y-1">
                  <div className="flex items-center gap-2">
                    <KeyRound className="h-4 w-4 text-xblue" />
                    <h2 className="text-lg font-semibold text-foreground">
                      {t("login.mfa.title")}
                    </h2>
                  </div>
                  <p className="text-xs text-muted-foreground">
                    {t("login.mfa.subTitle")}
                  </p>
                </div>

                <FormField
                  label={t("login.mfa.placeholder")}
                  error={mfaError}
                  required
                >
                  <Input
                    value={mfaCode}
                    autoFocus
                    maxLength={8}
                    autoComplete="one-time-code"
                    placeholder={t("login.mfa.placeholder")}
                    data-testid="mfa-code-input"
                    onChange={(e) => {
                      setMfaCode(e.target.value.trim().toUpperCase());
                      if (mfaError) setMfaError(undefined);
                    }}
                  />
                </FormField>

                <div className="space-y-2 pt-1">
                  <Button
                    type="submit"
                    variant="primary"
                    loading={mfaLoading}
                    data-testid="mfa-submit"
                    className="w-full"
                  >
                    {t("login.mfa.btnText")}
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={handleBackToLogin}
                    className="w-full"
                  >
                    <ArrowLeft className="h-3.5 w-3.5" />
                    {t("login.mfa.backToLogin")}
                  </Button>
                </div>
              </form>
            )}
          </Card>
        </div>
      </section>
    </div>
  );
}
