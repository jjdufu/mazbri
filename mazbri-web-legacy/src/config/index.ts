import { MenuThemeEnum, SystemThemeEnum } from "@/enums/appEnum";
import { SystemConfig } from "@/types/config";
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
  themeList: [
    {
      theme: MenuThemeEnum.DESIGN,
      background: "#0c1b3e",
      systemNameColor: "#ffffff",
      iconColor: "rgba(255, 255, 255, 0.7)",
      textColor: "rgba(255, 255, 255, 0.85)",
    },
    {
      theme: MenuThemeEnum.DARK,
      background: "#191A23",
      systemNameColor: "#D9DADB",
      iconColor: "#BABBBD",
      textColor: "#BABBBD",
    },
    {
      theme: MenuThemeEnum.LIGHT,
      background: "#0c1b3e",
      systemNameColor: "#ffffff",
      iconColor: "rgba(255, 255, 255, 0.7)",
      textColor: "rgba(255, 255, 255, 0.85)",
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
