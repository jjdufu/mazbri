import { useSettingStore } from "@/store/modules/setting";
import { SystemThemeEnum } from "@/enums/appEnum";
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
    const resolvedMode = themeMode ?? theme;
    settingStore.setGlopTheme(theme, resolvedMode);
    settingStore.applyToDom();
    if (typeof requestAnimationFrame !== "undefined") {
      requestAnimationFrame(() => {
        enableTransitions();
      });
    } else {
      enableTransitions();
    }
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
  return {
    setSystemTheme,
    setSystemAutoTheme,
    switchThemeStyles,
    prefersDark,
  };
}

export function initializeTheme() {
  const settingStore = useSettingStore();
  const prefersDark = usePreferredDark();
  const applyThemeByMode = () => {
    const el = document.documentElement;
    if (settingStore.systemThemeMode === SystemThemeEnum.AUTO) {
      const actualTheme = prefersDark.value
        ? SystemThemeEnum.DARK
        : SystemThemeEnum.LIGHT;
      settingStore.setGlopTheme(actualTheme, SystemThemeEnum.AUTO);
    }
    settingStore.applyToDom();
    const radiusRem = `${settingStore.customRadius}rem`;
    const isZero = parseFloat(String(settingStore.customRadius)) === 0;
    if (isZero) {
      el.classList.add("radius-zero");
    } else {
      el.classList.remove("radius-zero");
    }
    el.style.setProperty("--custom-radius", radiusRem);
    el.style.setProperty(
      "--el-border-radius-base",
      `calc(${radiusRem} / 3 + 2px)`,
    );
    el.style.setProperty(
      "--el-border-radius-small",
      `calc(${radiusRem} / 3 + 4px)`,
    );
    el.style.setProperty(
      "--el-messagebox-border-radius",
      `calc(${radiusRem} / 3 + 4px)`,
    );
    el.style.setProperty(
      "--el-popover-border-radius",
      `calc(${radiusRem} / 3 + 4px)`,
    );
    el.style.setProperty(
      "--el-card-border-radius",
      `calc(${radiusRem} + 4px)`,
    );
  };
  applyThemeByMode();
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
