import { defineStore } from "pinia";
import { ref, computed } from "vue";
import { usePreferredDark } from "@vueuse/core";
import { MenuThemeType } from "@/types/store";
import AppConfig from "@/config";
import { SystemThemeEnum, MenuThemeEnum } from "@/enums/appEnum";
import { setElementThemeColor } from "@/utils/ui";
import { StorageConfig } from "@/utils";
import { SETTING_DEFAULT_CONFIG } from "@/config/setting";
export const useThemeStore = defineStore("themeStore", () => {
  const prefersDark = usePreferredDark();
  const systemThemeType = ref(SETTING_DEFAULT_CONFIG.systemThemeType);
  const systemThemeMode = ref(SETTING_DEFAULT_CONFIG.systemThemeMode);
  const menuThemeType = ref(SETTING_DEFAULT_CONFIG.menuThemeType);
  const systemThemeColor = ref(SETTING_DEFAULT_CONFIG.systemThemeColor);
  const darkThemeStyle = ref<string>(SETTING_DEFAULT_CONFIG.darkThemeStyle);
  const isDark = computed((): boolean => {
    if (systemThemeMode.value === SystemThemeEnum.AUTO) {
      return prefersDark.value;
    }
    return systemThemeType.value === SystemThemeEnum.DARK;
  });
  const getMenuTheme = computed((): MenuThemeType => {
    const list = AppConfig.themeList.filter(
      (item) => item.theme === menuThemeType.value,
    );
    if (isDark.value) {
      return AppConfig.darkMenuStyles[0];
    }
    return list[0];
  });
  const setGlopTheme = (theme: SystemThemeEnum, themeMode: SystemThemeEnum) => {
    systemThemeType.value = theme;
    systemThemeMode.value = themeMode;
    localStorage.setItem(StorageConfig.THEME_KEY, theme);
  };
  const switchMenuStyles = (theme: MenuThemeEnum) => {
    menuThemeType.value = theme;
  };
  const setElementTheme = (theme: string) => {
    systemThemeColor.value = theme;
    setElementThemeColor(theme);
  };
  const applyThemeToDom = () => {
    const el = document.documentElement;
    let actualTheme = systemThemeType.value;
    if (systemThemeMode.value === SystemThemeEnum.AUTO) {
      const prefersDark = window.matchMedia(
        "(prefers-color-scheme: dark)",
      ).matches;
      actualTheme = prefersDark ? SystemThemeEnum.DARK : SystemThemeEnum.LIGHT;
      systemThemeType.value = actualTheme;
    }
    const themeClassName =
      actualTheme === SystemThemeEnum.DARK ? "dark" : "light";
    if (!el.classList.contains(themeClassName)) {
      el.classList.remove("dark", "light");
      el.classList.add(themeClassName);
    }
    if (actualTheme === SystemThemeEnum.DARK) {
      el.setAttribute("data-dark-theme", darkThemeStyle.value || "obsidian");
    } else {
      el.removeAttribute("data-dark-theme");
    }
    setElementThemeColor(systemThemeColor.value);
  };
  return {
    systemThemeType,
    systemThemeMode,
    menuThemeType,
    systemThemeColor,
    darkThemeStyle,
    isDark,
    getMenuTheme,
    setGlopTheme,
    switchMenuStyles,
    setElementTheme,
    applyThemeToDom,
  };
});
