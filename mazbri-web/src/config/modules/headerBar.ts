import { HeaderBarFeatureConfig } from "@/types";
import { $t } from "@/locales";
export const headerBarConfig: HeaderBarFeatureConfig = {
  menuButton: {
    enabled: true,
    description: $t("config.headerBar.menuButton"),
  },
  refreshButton: {
    enabled: true,
    description: $t("config.headerBar.refreshButton"),
  },
  fastEnter: {
    enabled: false,
    description: $t("config.headerBar.fastEnter"),
  },
  breadcrumb: {
    enabled: true,
    description: $t("config.headerBar.breadcrumb"),
  },
  fullscreen: {
    enabled: false,
    description: $t("config.headerBar.fullscreen"),
  },
  language: {
    enabled: false,
    description: $t("config.headerBar.language"),
  },
  themeToggle: {
    enabled: true,
    description: $t("config.headerBar.themeToggle"),
  },
};
export default headerBarConfig;
