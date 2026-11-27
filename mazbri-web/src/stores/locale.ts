import { create } from "zustand";
import { LanguageEnum } from "@/enums/appEnum";
import { StorageKeyManager } from "@/utils/storage/storage-key-manager";

const LOCALE_STORAGE_KEY = "mazbri-locale";

function isLanguageEnum(value: unknown): value is LanguageEnum {
  return (
    value === LanguageEnum.ZH ||
    value === LanguageEnum.EN
  );
}

function readInitialLocale(): LanguageEnum {
  if (typeof localStorage === "undefined") {
    return LanguageEnum.ZH;
  }
  try {
    const direct = localStorage.getItem(LOCALE_STORAGE_KEY);
    if (isLanguageEnum(direct)) {
      return direct;
    }
    const storageKey = new StorageKeyManager().getStorageKey("user");
    const legacyRaw = localStorage.getItem(storageKey);
    if (legacyRaw) {
      const parsed = JSON.parse(legacyRaw) as { language?: unknown };
      if (isLanguageEnum(parsed.language)) {
        return parsed.language;
      }
    }
  } catch {
    // Fallback to ZH when storage is unavailable or malformed.
  }
  return LanguageEnum.ZH;
}

export interface LocaleState {
  locale: LanguageEnum;
  setLocale: (locale: LanguageEnum) => void;
  toggleLocale: () => void;
}

const initialLocale = readInitialLocale();
if (typeof document !== "undefined") {
  document.documentElement.lang =
    initialLocale === LanguageEnum.EN ? "en" : "zh-CN";
}

export const useLocaleStore = create<LocaleState>((set, get) => ({
  locale: initialLocale,
  setLocale: (locale: LanguageEnum) => {
    if (!isLanguageEnum(locale)) return;
    if (typeof localStorage !== "undefined") {
      try {
        localStorage.setItem(LOCALE_STORAGE_KEY, locale);
      } catch {
        // Ignore storage quota / privacy mode errors.
      }
    }
    if (typeof document !== "undefined") {
      document.documentElement.lang = locale === LanguageEnum.EN ? "en" : "zh-CN";
    }
    set({ locale });
  },
  toggleLocale: () => {
    const next =
      get().locale === LanguageEnum.ZH ? LanguageEnum.EN : LanguageEnum.ZH;
    get().setLocale(next);
  },
}));
