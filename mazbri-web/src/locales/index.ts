import { createI18n } from "vue-i18n";
import type { I18n, I18nOptions } from "vue-i18n";
import { LanguageEnum } from "@/enums/appEnum";
import { getSystemStorage } from "@/utils/storage";
import { StorageKeyManager } from "@/utils/storage/storage-key-manager";
import { logger } from "@/utils/sys/logger";
import { ErrorCodes } from "@/constants/error-codes";
import enMessages from "./langs/en.json";
import zhMessages from "./langs/zh.json";
const storageKeyManager = new StorageKeyManager();
const messages = {
  [LanguageEnum.EN]: enMessages,
  [LanguageEnum.ZH]: zhMessages,
};
export const languageOptions = [
  { value: LanguageEnum.ZH, labelKey: "common.languages.zh" },
  { value: LanguageEnum.EN, labelKey: "common.languages.en" },
];
const getDefaultLanguage = (): LanguageEnum => {
  try {
    const storageKey = storageKeyManager.getStorageKey("user");
    const userStore = localStorage.getItem(storageKey);
    if (userStore) {
      const { language } = JSON.parse(userStore);
      if (language && Object.values(LanguageEnum).includes(language)) {
        return language;
      }
    }
  } catch (error) {
    logger.warn(
      "[i18n] Failed to read language setting from versioned storage",
      {
        service: "mazbri-mgmt-web",
        event: "i18n_storage_read_fallback",
        severity: "warn",
        error_code: ErrorCodes.INTERNAL_ERROR,
        error,
      },
    );
  }
  try {
    const sys = getSystemStorage();
    if (sys) {
      const user = (
        sys as {
          user?: {
            language?: unknown;
          };
        }
      ).user;
      if (
        typeof user?.language === "string" &&
        Object.values(LanguageEnum).includes(user.language as LanguageEnum)
      ) {
        return user.language as LanguageEnum;
      }
    }
  } catch (error) {
    logger.warn("[i18n] Failed to read language setting from system storage", {
      service: "mazbri-mgmt-web",
      event: "i18n_system_storage_fallback",
      severity: "warn",
      error_code: ErrorCodes.INTERNAL_ERROR,
      error,
    });
  }
  return LanguageEnum.ZH;
};
const i18nOptions: I18nOptions = {
  locale: getDefaultLanguage(),
  legacy: false,
  globalInjection: true,
  fallbackLocale: LanguageEnum.ZH,
  messages,
};
const i18n: I18n = createI18n(i18nOptions);
interface Translation {
  (key: string, named?: Record<string, unknown>): string;
}
export const $t = i18n.global.t as Translation;
export default i18n;
