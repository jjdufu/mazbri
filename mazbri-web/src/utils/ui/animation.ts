import { SystemThemeEnum } from "@/enums/appEnum";
import { useSettingStore } from "@/store/modules/setting";
import { useTheme } from "@/hooks/core/useTheme";
const { LIGHT, DARK } = SystemThemeEnum;
export const themeAnimation = (_e: MouseEvent): void => {
  if (document.startViewTransition) {
    document.startViewTransition(() => toggleTheme());
  } else {
    toggleTheme();
  }
};
const toggleTheme = () => {
  const settingStore = useSettingStore();
  const currentTheme = document.documentElement.classList.contains("dark")
    ? DARK
    : document.documentElement.classList.contains("light")
      ? LIGHT
      : settingStore.isDark
        ? DARK
        : LIGHT;
  useTheme().switchThemeStyles(currentTheme === LIGHT ? DARK : LIGHT);
};
export const toggleTransition = (enable: boolean) => {
  const body = document.body;
  if (enable) {
    body.classList.add("theme-change");
  } else {
    setTimeout(() => {
      body.classList.remove("theme-change");
    }, 300);
  }
};
