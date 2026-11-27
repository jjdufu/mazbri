import { MenuThemeEnum, SystemThemeEnum } from "@/enums/appEnum";
import { LocationQueryRaw } from "vue-router";
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
export interface SettingState {
  theme: string;
  menuButton: boolean;
  showRefreshButton: boolean;
  showCrumbs: boolean;
  autoClose: boolean;
  showWorkTab: boolean;
  showLanguage: boolean;
  showNprogress: boolean;
  themeModel: string;
}
export interface WorkTab {
  title: string;
  customTitle?: string;
  path: string;
  name: string;
  keepAlive: boolean;
  fixedTab?: boolean;
  params?: object;
  query?: LocationQueryRaw;
  icon?: string;
  isActive?: boolean;
}
export interface UserState {
  userInfo: Api.Auth.UserInfo | null;
  token: string | null;
  roles: string[];
  permissions: string[];
}
export interface SettingStoreState extends SettingState {
  collapsed: boolean;
  device: "desktop" | "mobile";
  language: string;
}
export interface WorkTabState {
  tabs: WorkTab[];
  activeTab: string;
  cachedTabs: string[];
}
export interface MenuState {
  menuList: any[];
  isLoaded: boolean;
  collapsed: boolean;
}
export interface RootState {
  user: UserState;
  setting: SettingStoreState;
  workTab: WorkTabState;
  menu: MenuState;
}
