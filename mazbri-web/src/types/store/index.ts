import type { MenuThemeEnum, SystemThemeEnum } from "@/enums/appEnum";

export interface SystemThemeType {
  className: string;
}

export type SystemThemeTypes = {
  [key in Exclude<SystemThemeEnum, SystemThemeEnum.AUTO>]: SystemThemeType;
};

export interface MenuThemeType {
  theme: MenuThemeEnum;
  background: string;
  systemNameColor: string;
  textColor: string;
  iconColor: string;
  img?: string;
}
