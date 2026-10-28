import { defineStore } from "pinia";
import { ref, computed } from "vue";
import { LanguageEnum } from "@/enums/appEnum";
import { router } from "@/router";
import { useSettingStore } from "./setting";
import { useWorktabStore } from "./worktab";
import { AppRouteRecord } from "@/types/router";
import { setPageTitle } from "@/utils/router";
import { resetRouterState } from "@/router/guards/beforeEach";
import { useMenuStore } from "./menu";
import { StorageConfig } from "@/utils/storage/storage-config";
import type { AdminUser, AdminRole } from "@/types/api";
import { logout as apiLogout } from "@/api/auth";
import { useMfaStepUpStore } from "./mfa-step-up";
export const useUserStore = defineStore(
  "userStore",
  () => {
    const language = ref(LanguageEnum.ZH);
    const isLogin = ref(false);
    const isLock = ref(false);
    const lockPassword = ref("");
    const info = ref<Partial<AdminUser>>({});
    const searchHistory = ref<AppRouteRecord[]>([]);
    const accessToken = ref("");
    const tokenExpiresAt = ref(0);
    const refreshToken = ref("");
    const getUserInfo = computed(() => info.value);
    const getSettingState = computed(() => useSettingStore().$state);
    const getWorktabState = computed(() => useWorktabStore().$state);
    const isAdmin = computed(() => info.value.role === "R_ADMIN");
    // 超级管理员（R_ADMIN）与管理员（R_FINANCE）拥有全部业务读写权限，与后端 route_access 的 BIZ 一致。
    const canWriteBusiness = computed(
      () => info.value.role === "R_ADMIN" || info.value.role === "R_FINANCE",
    );
    const needsPasswordReset = computed(() => info.value.force_password_reset);
    const setUserInfo = (newInfo: AdminUser) => {
      info.value = {
        ...newInfo,
        userName: newInfo.username,
        roles: [newInfo.role],
        buttons: [],
        isSuperAdmin: newInfo.role === "R_ADMIN",
      };
    };
    const setLoginStatus = (status: boolean) => {
      isLogin.value = status;
    };
    const setLanguage = (lang: LanguageEnum) => {
      setPageTitle(router.currentRoute.value);
      language.value = lang;
    };
    const setSearchHistory = (list: AppRouteRecord[]) => {
      searchHistory.value = list;
    };
    const setLockStatus = (status: boolean) => {
      isLock.value = status;
    };
    async function sha256(message: string): Promise<string> {
      const msgBuffer = new TextEncoder().encode(message);
      const subtleCrypto =
        typeof globalThis.crypto !== "undefined"
          ? globalThis.crypto.subtle
          : undefined;


      if (!subtleCrypto) {
        throw new Error(
          "Web Crypto API (crypto.subtle) is required for lock-screen password hashing. Use HTTPS or localhost.",
        );
      }
      const hashBuffer = await subtleCrypto.digest("SHA-256", msgBuffer);
      const hashArray = Array.from(new Uint8Array(hashBuffer));
      return hashArray.map((b) => b.toString(16).padStart(2, "0")).join("");
    }
    const setLockPassword = async (password: string) => {
      lockPassword.value = await sha256(password);
    };
    const isLockPasswordMatch = async (input: string): Promise<boolean> => {
      return (await sha256(input)) === lockPassword.value;
    };





    const setToken = (
      _newAccessToken?: string,
      expiresIn?: number,
      _newRefreshToken?: string,
    ) => {
      accessToken.value = "";
      refreshToken.value = "";
      if (expiresIn) {
        tokenExpiresAt.value = Math.floor(Date.now() / 1000) + expiresIn;
      }
    };
    const isTokenExpired = () => {
      if (!tokenExpiresAt.value) return false;
      return Math.floor(Date.now() / 1000) >= tokenExpiresAt.value - 60;
    };
    const clearUserState = () => {
      const currentUserId = info.value.id;
      if (currentUserId) {
        localStorage.setItem(
          StorageConfig.LAST_USER_ID_KEY,
          String(currentUserId),
        );
      }
      info.value = {};
      isLogin.value = false;
      isLock.value = false;
      lockPassword.value = "";
      accessToken.value = "";
      refreshToken.value = "";
      tokenExpiresAt.value = 0;
      sessionStorage.removeItem("iframeRoutes");
      useMenuStore().setHomePath("");
    };
    const logOut = async () => {

      try {
        await apiLogout({});
      } catch {
        void 0;
      }
      useMfaStepUpStore().reset();
      clearUserState();
      resetRouterState(500);
      const currentRoute = router.currentRoute.value;
      const redirect =
        currentRoute.path !== "/login" ? currentRoute.fullPath : undefined;
      router.push({
        name: "Login",
        query: redirect ? { redirect } : undefined,
      });
    };
    const checkAndClearWorktabs = () => {
      const lastUserId = localStorage.getItem(StorageConfig.LAST_USER_ID_KEY);
      const currentUserId = info.value.id;
      if (!currentUserId) return;
      if (!lastUserId) {
        return;
      }
      if (String(currentUserId) !== lastUserId) {
        const worktabStore = useWorktabStore();
        worktabStore.clearAll();
      }
      localStorage.removeItem(StorageConfig.LAST_USER_ID_KEY);
    };
    return {
      language,
      isLogin,
      isLock,
      lockPassword,
      info,
      searchHistory,
      accessToken,
      tokenExpiresAt,
      refreshToken,
      getUserInfo,
      getSettingState,
      getWorktabState,
      isAdmin,
      canWriteBusiness,
      needsPasswordReset,
      setUserInfo,
      setLoginStatus,
      setLanguage,
      setSearchHistory,
      setLockStatus,
      setLockPassword,
      isLockPasswordMatch,
      setToken,
      isTokenExpired,
      clearUserState,
      logOut,
      checkAndClearWorktabs,
    };
  },
  {
    persist: {
      key: "user",
      storage: localStorage,

      pick: ["isLogin", "language", "info", "tokenExpiresAt"],
    },
  },
);
