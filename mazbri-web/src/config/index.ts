import { MenuThemeEnum, MenuTypeEnum, SystemThemeEnum } from "@/enums/appEnum";
import { SystemConfig } from "@/types/config";
import { configImages } from "./assets/images";
import { headerBarConfig } from "./modules/headerBar";
const appConfig: SystemConfig = {
  systemInfo: {
    name: "Mazbri",
    siteUrl: "",
  },
  systemThemeStyles: {
    [SystemThemeEnum.LIGHT]: { className: "" },
    [SystemThemeEnum.DARK]: { className: SystemThemeEnum.DARK },
  },
  settingThemeList: [
    {
      name: "Light",
      theme: SystemThemeEnum.LIGHT,
      color: ["#fff", "#fff"],
      leftLineColor: "#EDEEF0",
      rightLineColor: "#EDEEF0",
      img: configImages.themeStyles.light,
    },
    {
      name: "Dark",
      theme: SystemThemeEnum.DARK,
      color: ["#22252A"],
      leftLineColor: "#3F4257",
      rightLineColor: "#3F4257",
      img: configImages.themeStyles.dark,
    },
    {
      name: "System",
      theme: SystemThemeEnum.AUTO,
      color: ["#fff", "#22252A"],
      leftLineColor: "#EDEEF0",
      rightLineColor: "#3F4257",
      img: configImages.themeStyles.system,
    },
  ],
  menuLayoutList: [
    {
      name: "Left",
      value: MenuTypeEnum.LEFT,
      img: configImages.menuLayouts.vertical,
    },
    {
      name: "Top",
      value: MenuTypeEnum.TOP,
      img: configImages.menuLayouts.horizontal,
    },
    {
      name: "Mixed",
      value: MenuTypeEnum.TOP_LEFT,
      img: configImages.menuLayouts.mixed,
    },
    {
      name: "Dual Column",
      value: MenuTypeEnum.DUAL_MENU,
      img: configImages.menuLayouts.dualColumn,
    },
  ],
  themeList: [
    {
      theme: MenuThemeEnum.DESIGN,
      background: "#0c1b3e",
      systemNameColor: "#ffffff",
      iconColor: "rgba(255, 255, 255, 0.7)",
      textColor: "rgba(255, 255, 255, 0.85)",
      img: configImages.menuStyles.design,
    },
    {
      theme: MenuThemeEnum.DARK,
      background: "#191A23",
      systemNameColor: "#D9DADB",
      iconColor: "#BABBBD",
      textColor: "#BABBBD",
      img: configImages.menuStyles.dark,
    },
    {
      theme: MenuThemeEnum.LIGHT,
      background: "#0c1b3e",
      systemNameColor: "#ffffff",
      iconColor: "rgba(255, 255, 255, 0.7)",
      textColor: "rgba(255, 255, 255, 0.85)",
      img: configImages.menuStyles.light,
    },
  ],
  darkMenuStyles: [
    {
      theme: MenuThemeEnum.DARK,
      background: "var(--default-box-color)",
      systemNameColor: "#DDDDDD",
      iconColor: "#BABBBD",
      textColor: "rgba(#FFFFFF, 0.7)",
    },
  ],
  systemMainColor: [
    "#5D87FF",
    "#B48DF3",
    "#1D84FF",
    "#60C041",
    "#38C0FC",
    "#F9901F",
    "#FF80C8",
  ] as const,
  headerBar: headerBarConfig,
};
export default appConfig;
