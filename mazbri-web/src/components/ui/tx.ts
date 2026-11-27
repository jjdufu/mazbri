import { useCallback } from "react";
import { LanguageEnum } from "@/enums/appEnum";
import { useT } from "@/locales";

export function useUiTx() {
  const { locale } = useT();
  const tx = useCallback(
    (zh: string, en: string): string => (locale === LanguageEnum.ZH ? zh : en),
    [locale],
  );
  return { locale, tx };
}
