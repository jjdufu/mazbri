import { useCallback } from "react";
import { LanguageEnum } from "@/enums/appEnum";
import { useLocaleStore } from "@/stores/locale";
import enMessages from "./langs/en.json";
import zhMessages from "./langs/zh.json";

type MessageDict = Record<string, unknown>;

const dictionaries: Record<LanguageEnum, MessageDict> = {
  [LanguageEnum.ZH]: zhMessages as MessageDict,
  [LanguageEnum.EN]: enMessages as MessageDict,
};

export const languageOptions = [
  { value: LanguageEnum.ZH, labelKey: "common.languages.zh" },
  { value: LanguageEnum.EN, labelKey: "common.languages.en" },
] as const;

function resolvePath(dict: MessageDict, key: string): string | undefined {
  let cursor: unknown = dict;
  for (const part of key.split(".")) {
    if (cursor && typeof cursor === "object" && !Array.isArray(cursor)) {
      cursor = (cursor as Record<string, unknown>)[part];
    } else {
      return undefined;
    }
  }
  return typeof cursor === "string" ? cursor : undefined;
}

function interpolate(
  template: string,
  named?: Record<string, unknown>,
): string {
  if (!named) return template;
  return template.replace(/\{(\w+)\}/g, (match, token: string) => {
    if (!(token in named)) return match;
    const value = named[token];
    return value === undefined || value === null ? "" : String(value);
  });
}

export function translate(
  locale: LanguageEnum,
  key: string,
  named?: Record<string, unknown>,
): string {
  const primary = resolvePath(dictionaries[locale] ?? dictionaries[LanguageEnum.ZH], key);
  const resolved =
    primary ?? resolvePath(dictionaries[LanguageEnum.ZH], key) ?? key;
  return interpolate(resolved, named);
}

/** Imperative translation helper (reads current reactive Zustand locale). */
export function $t(key: string, named?: Record<string, unknown>): string {
  return translate(useLocaleStore.getState().locale, key, named);
}

export const t = $t;

export function getLocale(): LanguageEnum {
  return useLocaleStore.getState().locale;
}

export function setLocale(locale: LanguageEnum): void {
  useLocaleStore.getState().setLocale(locale);
}

/** Reactive React hook for hot-switching zh/en without page reload (U4). */
export function useT() {
  const locale = useLocaleStore((s) => s.locale);
  const setLocaleFn = useLocaleStore((s) => s.setLocale);
  const toggleLocale = useLocaleStore((s) => s.toggleLocale);
  const boundT = useCallback(
    (key: string, named?: Record<string, unknown>) =>
      translate(locale, key, named),
    [locale],
  );
  return {
    t: boundT,
    locale,
    setLocale: setLocaleFn,
    toggleLocale,
  };
}

/** Register additional domain-scoped messages into the runtime dictionary. */
export function mergeLocaleMessages(
  locale: LanguageEnum,
  extra: MessageDict,
): void {
  const target = dictionaries[locale];
  const merge = (dst: MessageDict, src: MessageDict) => {
    for (const [k, v] of Object.entries(src)) {
      if (
        v &&
        typeof v === "object" &&
        !Array.isArray(v) &&
        dst[k] &&
        typeof dst[k] === "object" &&
        !Array.isArray(dst[k])
      ) {
        merge(dst[k] as MessageDict, v as MessageDict);
      } else {
        dst[k] = v;
      }
    }
  };
  merge(target, extra);
}

export default {
  t: $t,
  useT,
  getLocale,
  setLocale,
};
