import type { ISODateString } from "./common";
import type { SystemThemeEnum } from "@/enums/appEnum";
export interface AppearanceSettings {
  system_theme_mode: SystemThemeEnum;
  system_theme_color: string;
  container_width: "100%" | "1200px";
  box_border_mode: boolean;
  menu_open_width: number;
  show_work_tab: boolean;
  show_menu_button: boolean;
  show_fast_enter: boolean;
  show_refresh_button: boolean;
  show_crumbs: boolean;
  show_language: boolean;
  show_nprogress: boolean;
  watermark_visible: boolean;
  tab_style?: string;
  page_transition?: string;
  custom_radius?: string;
  dark_theme_style: "obsidian" | "graphite" | "oceanic" | "nordic";
  updated_at?: ISODateString;
}
export interface UpdateAppearanceRequest {
  system_theme_mode?: SystemThemeEnum;
  system_theme_color?: string;
  container_width?: "100%" | "1200px";
  box_border_mode?: boolean;
  menu_open_width?: number;
  show_work_tab?: boolean;
  show_menu_button?: boolean;
  show_fast_enter?: boolean;
  show_refresh_button?: boolean;
  show_crumbs?: boolean;
  show_language?: boolean;
  show_nprogress?: boolean;
  watermark_visible?: boolean;
  tab_style?: string;
  page_transition?: string;
  custom_radius?: string;
  dark_theme_style?: "obsidian" | "graphite" | "oceanic" | "nordic";
}
