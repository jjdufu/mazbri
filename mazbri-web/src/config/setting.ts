import AppConfig from "@/config";
import {
  SystemThemeEnum,
  MenuThemeEnum,
  MenuTypeEnum,
  ContainerWidthEnum,
} from "@/enums/appEnum";
export const SETTING_DEFAULT_CONFIG = {
  menuType: MenuTypeEnum.LEFT,
  menuOpenWidth: 200,
  menuOpen: true,
  dualMenuShowText: false,
  systemThemeType: SystemThemeEnum.LIGHT,
  systemThemeMode: SystemThemeEnum.LIGHT,
  menuThemeType: MenuThemeEnum.DESIGN,
  systemThemeColor: AppConfig.systemMainColor[0],
  showMenuButton: true,
  showFastEnter: true,
  showRefreshButton: true,
  showCrumbs: true,
  showWorkTab: false,
  showLanguage: true,
  showNprogress: true,
  showFestivalText: false,
  watermarkVisible: false,
  autoClose: false,
  refresh: false,
  holidayFireworksLoaded: false,
  boxBorderMode: false,
  pageTransition: "fade",
  tabStyle: "tab-default",
  customRadius: "1",
  darkThemeStyle: "obsidian",
  containerWidth: ContainerWidthEnum.FULL,
  festivalDate: "",
};
export function getSettingDefaults() {
  return { ...SETTING_DEFAULT_CONFIG };
}
export function resetToDefaults(currentSettings: Record<string, any>) {
  const defaults = getSettingDefaults();
  Object.keys(defaults).forEach((key) => {
    if (key in currentSettings) {
      currentSettings[key] = defaults[key as keyof typeof defaults];
    }
  });
}
