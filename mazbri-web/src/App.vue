<template>
  <ElConfigProvider
    size="default"
    :locale="locales[language]"
    :z-index="3000"
    :message="messageConfig"
  >
    <RouterView></RouterView>
  </ElConfigProvider>
</template>

<script setup lang="ts">
import { useUserStore } from "./store/modules/user";
import { useMfaStepUpStore } from "./store/modules/mfa-step-up";
import { useSettingStore } from "./store/modules/setting";
import zh from "element-plus/es/locale/lang/zh-cn";
import en from "element-plus/es/locale/lang/en";
import { systemUpgrade } from "./utils/sys";
import { toggleTransition } from "./utils/ui/animation";
import { checkStorageCompatibility } from "./utils/storage";
import { initializeTheme } from "./hooks/core/useTheme";
import { logger } from "./utils/sys/logger";
import { ErrorCodes } from "./constants/error-codes";
const router = useRouter();
const userStore = useUserStore();
const mfaStepUpStore = useMfaStepUpStore();
const settingStore = useSettingStore();
const { language } = storeToRefs(userStore);
const locales = {
  zh: zh,
  en: en,
};
// 全局消息：相同文案合并显示，同时最多 3 条，避免叠满页面
const messageConfig = { max: 3, grouping: true };
const initializeSettings = async () => {
  if (userStore.isLogin) {
    try {
      await settingStore.loadFromServer();
    } catch (error) {
      logger.warn(
        "[App] Failed to load settings from server; using local storage",
        {
          service: "mazbri-mgmt-web",
          event: "app_settings_load_fallback",
          severity: "warn",
          error_code: ErrorCodes.INTERNAL_ERROR,
          error,
        },
      );
    }
    // 会话可能已失效（localStorage 仍标登录）；登出过程中也不再打 step-up
    if (userStore.isLogin && !userStore.isLoggingOut) {
      try {
        await mfaStepUpStore.refresh();
      } catch {
        // refresh 已记录日志。已登录初始化只同步提权状态，不开窗。
      }
    }
  }
  settingStore.applyToDom();
};
onBeforeMount(() => {
  toggleTransition(true);
  initializeTheme();
});
onMounted(async () => {
  checkStorageCompatibility();
  toggleTransition(false);
  systemUpgrade();
  await initializeSettings();
});
router.afterEach(() => {
  settingStore.applyToDom();
});
</script>
