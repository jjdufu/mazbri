import { HeaderBarFeatureConfig } from "@/types";
export const headerBarConfig: HeaderBarFeatureConfig = {
  menuButton: {
    enabled: true,
    description: "config.headerBar.menuButton",
  },
  refreshButton: {
    enabled: true,
    description: "config.headerBar.refreshButton",
  },
  fastEnter: {
    enabled: false,
    description: "config.headerBar.fastEnter",
  },
  breadcrumb: {
    enabled: true,
    description: "config.headerBar.breadcrumb",
  },
  fullscreen: {
    enabled: false,
    description: "config.headerBar.fullscreen",
  },
  language: {
    enabled: true,
    description: "config.headerBar.language",
  },
  themeToggle: {
    enabled: true,
    description: "config.headerBar.themeToggle",
  },
};
export default headerBarConfig;
