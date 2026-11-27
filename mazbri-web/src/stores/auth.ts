import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { AdminUser } from "@/types/api";
import { StorageConfig } from "@/utils/storage/storage-config";
import { resetCaCertificateCache } from "@/api/certificate";
import { resetCanonicalServiceNamesCache } from "@/api/service-names";
import { resetSystemSettingsCache } from "@/api/settings";
import { useLockScreenStore } from "@/stores/lockscreen";
import { usePersonalSecurityStore } from "@/stores/personal-security";
import { useWatermarkStore } from "@/stores/watermark";

export type AuthUserInfo = Partial<AdminUser> & {
  userName?: string;
  roles?: string[];
  buttons?: string[];
  isSuperAdmin?: boolean;
};

type LogoutRedirectHandler = () => void;
let logoutRedirectHandler: LogoutRedirectHandler | null = null;

/** Stage 2 router registers the login redirect callback here to avoid circular imports. */
export function registerAuthLogoutRedirect(
  handler: LogoutRedirectHandler | null,
): void {
  logoutRedirectHandler = handler;
}

export interface AuthState {
  isLogin: boolean;
  isLoggingOut: boolean;
  info: AuthUserInfo;
  tokenExpiresAt: number;
  setUserInfo: (newInfo: AdminUser) => void;
  setLoginStatus: (status: boolean) => void;
  setToken: (
    accessToken?: string,
    expiresIn?: number,
    refreshToken?: string,
  ) => void;
  isTokenExpired: () => boolean;
  clearUserState: () => void;
  logOut: (options?: { skipRemote?: boolean }) => Promise<void>;
}

export const useAuthStore = create<AuthState>()(
  persist(
    (set, get) => ({
      isLogin: false,
      isLoggingOut: false,
      info: {},
      tokenExpiresAt: 0,
      setUserInfo: (newInfo: AdminUser) => {
        set({
          info: {
            ...newInfo,
            userName: newInfo.username,
            roles: [newInfo.role],
            buttons: [],
            isSuperAdmin: newInfo.role === "R_ADMIN",
          },
        });
      },
      setLoginStatus: (status: boolean) => {
        set({ isLogin: status });
      },
      setToken: (
        _accessToken?: string,
        expiresIn?: number,
        _refreshToken?: string,
      ) => {
        if (expiresIn) {
          set({ tokenExpiresAt: Math.floor(Date.now() / 1000) + expiresIn });
        }
      },
      isTokenExpired: () => {
        const expiresAt = get().tokenExpiresAt;
        if (!expiresAt) return false;
        return Math.floor(Date.now() / 1000) >= expiresAt - 60;
      },
      clearUserState: () => {
        const currentUserId = get().info.id;
        if (currentUserId && typeof localStorage !== "undefined") {
          try {
            localStorage.setItem(
              StorageConfig.LAST_USER_ID_KEY,
              String(currentUserId),
            );
          } catch {
            // Ignore storage errors in restricted contexts.
          }
        }
        set({
          info: {},
          isLogin: false,
          tokenExpiresAt: 0,
        });
        resetCanonicalServiceNamesCache();
        resetCaCertificateCache();
        resetSystemSettingsCache();
        useLockScreenStore.getState().reset();
        usePersonalSecurityStore.getState().closeDrawer();
        useWatermarkStore.getState().reset();
      },
      logOut: async (options?: { skipRemote?: boolean }) => {
        set({ isLoggingOut: true });
        try {
          const { useMfaStepUpStore } = await import("@/stores/mfa-step-up");
          useMfaStepUpStore.getState().reset();
          if (!options?.skipRemote) {
            try {
              const { logout: apiLogout } = await import("@/api/auth");
              await apiLogout({});
            } catch {
              // Ignore remote logout errors when tearing down local session.
            }
          }
          get().clearUserState();
          logoutRedirectHandler?.();
        } finally {
          set({ isLoggingOut: false });
        }
      },
    }),
    {
      name: "mazbri-auth",
      partialize: (state) => ({
        isLogin: state.isLogin,
        info: state.info,
        tokenExpiresAt: state.tokenExpiresAt,
      }),
    },
  ),
);
