import { useSettingStore } from "@/store/modules/setting";
import { SystemThemeEnum } from "@/enums/appEnum";
import AppConfig from "@/config";
import { SystemThemeTypes } from "@/types/store";
import { getDarkColor, getLightColor, setElementThemeColor } from "@/utils/ui";
import { usePreferredDark } from "@vueuse/core";
import { watch } from "vue";
export function useTheme() {
  const settingStore = useSettingStore();
  const disableTransitions = () => {
    document.documentElement.classList.add("theme-change");
  };
  const enableTransitions = () => {
    document.documentElement.classList.remove("theme-change");
  };
  const setSystemTheme = (
    theme: SystemThemeEnum,
    themeMode?: SystemThemeEnum,
  ) => {
    disableTransitions();
    const el = document.documentElement;
    const isDark = theme === SystemThemeEnum.DARK;
    if (!themeMode) {
      themeMode = theme;
    }
    const currentTheme =
      AppConfig.systemThemeStyles[theme as keyof SystemThemeTypes];
    const primary = settingStore.systemThemeColor;
    const colorVariables: Array<{
      name: string;
      value: string;
    }> = [];
    for (let i = 1; i <= 9; i++) {
      colorVariables.push({
        name: `--el-color-primary-light-${i}`,
        value: isDark
          ? getDarkColor(primary, i / 10)
          : getLightColor(primary, i / 10),
      });
    }
    requestAnimationFrame(() => {
      if (currentTheme) {
        el.setAttribute("class", currentTheme.className);
      }
      colorVariables.forEach(({ name, value }) => {
        el.style.setProperty(name, value);
      });
      settingStore.setGlopTheme(theme, themeMode);
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          requestAnimationFrame(() => {
            enableTransitions();
          });
        });
      });
    });
  };
  const prefersDark = usePreferredDark();
  const setSystemAutoTheme = () => {
    const theme = prefersDark.value
      ? SystemThemeEnum.DARK
      : SystemThemeEnum.LIGHT;
    setSystemTheme(theme, SystemThemeEnum.AUTO);
  };
  const switchThemeStyles = (theme: SystemThemeEnum) => {
    if (theme === SystemThemeEnum.AUTO) {
      setSystemAutoTheme();
    } else {
      setSystemTheme(theme);
    }
  };
  const switchThemeStylesAndPersist = async (theme: SystemThemeEnum) => {
    switchThemeStyles(theme);
    await settingStore.updateServerSettings({
      system_theme_mode: theme,
    });
  };
  return {
    setSystemTheme,
    setSystemAutoTheme,
    switchThemeStyles,
    switchThemeStylesAndPersist,
    prefersDark,
  };
}
export function initializeTheme() {
  const settingStore = useSettingStore();
  const prefersDark = usePreferredDark();
  const applyThemeByMode = () => {
    const el = document.getElementsByTagName("html")[0];
    let actualTheme = settingStore.systemThemeType;
    if (settingStore.systemThemeMode === SystemThemeEnum.AUTO) {
      actualTheme = prefersDark.value
        ? SystemThemeEnum.DARK
        : SystemThemeEnum.LIGHT;
      settingStore.systemThemeType = actualTheme;
    }
    const currentTheme =
      AppConfig.systemThemeStyles[actualTheme as keyof SystemThemeTypes];
    if (currentTheme) {
      el.setAttribute("class", currentTheme.className);
    }
    setElementThemeColor(settingStore.systemThemeColor);
    const radiusRem = `${settingStore.customRadius}rem`;
    const isZero = parseFloat(String(settingStore.customRadius)) === 0;
    if (isZero) {
      document.documentElement.classList.add("radius-zero");
    } else {
      document.documentElement.classList.remove("radius-zero");
    }
    document.documentElement.style.setProperty("--custom-radius", radiusRem);
    document.documentElement.style.setProperty(
      "--el-border-radius-base",
      `calc(${radiusRem} / 3 + 2px)`,
    );
    document.documentElement.style.setProperty(
      "--el-border-radius-small",
      `calc(${radiusRem} / 3 + 4px)`,
    );
    document.documentElement.style.setProperty(
      "--el-messagebox-border-radius",
      `calc(${radiusRem} / 3 + 4px)`,
    );
    document.documentElement.style.setProperty(
      "--el-popover-border-radius",
      `calc(${radiusRem} / 3 + 4px)`,
    );
    document.documentElement.style.setProperty(
      "--el-card-border-radius",
      `calc(${radiusRem} + 4px)`,
    );
  };
  applyThemeByMode();
  if (settingStore.systemThemeMode === SystemThemeEnum.AUTO) {
    watch(
      prefersDark,
      () => {
        if (settingStore.systemThemeMode === SystemThemeEnum.AUTO) {
          applyThemeByMode();
        }
      },
      { immediate: false },
    );
  }
}
