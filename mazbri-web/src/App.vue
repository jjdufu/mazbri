<template>
  <ElConfigProvider size="default" :locale="locales[language]" :z-index="3000">
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
import { startExpandOnFocusInputAuto } from "./composables/useExpandOnFocusInputs";
const router = useRouter();
const userStore = useUserStore();
const mfaStepUpStore = useMfaStepUpStore();
const settingStore = useSettingStore();
const { language } = storeToRefs(userStore);
const locales = {
  zh: zh,
  en: en,
};
let stopExpandOnFocusAuto: (() => void) | undefined;
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
    try {
      await mfaStepUpStore.refresh();
    } catch {
      // refresh 已记录日志。已登录初始化只同步提权状态，不开窗。
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

  stopExpandOnFocusAuto = startExpandOnFocusInputAuto({
    maxCompactWidth: 120,
    expandWidth: 360,
  });
});
onBeforeUnmount(() => {
  stopExpandOnFocusAuto?.();
});
router.afterEach(() => {
  settingStore.applyToDom();
});
</script>
