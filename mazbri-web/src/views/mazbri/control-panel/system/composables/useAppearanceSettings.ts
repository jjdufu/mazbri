import { ref, reactive, computed, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useSettingStore } from "@/store/modules/setting";
import { ContainerWidthEnum, SystemThemeEnum } from "@/enums/appEnum";
import { headerBarConfig } from "@/config/modules/headerBar";
import { ElMessage, ElMessageBox } from "element-plus";
import { logger } from "@/utils/sys";
import type { UpdateAppearanceRequest } from "@/types/api/appearance";
import { useTheme } from "@/hooks/core/useTheme";
import { useAppearanceSettingsConfig } from "./useAppearanceSettingsConfig";
export interface LocalAppearanceSettings {
  systemThemeMode: SystemThemeEnum;
  systemThemeColor: string;
  containerWidth: "100%" | "1200px";
  boxBorderMode: boolean;
  showWorkTab: boolean;
  showMenuButton: boolean;
  showRefreshButton: boolean;
  showCrumbs: boolean;
  showLanguage: boolean;
  showNprogress: boolean;
  watermarkVisible: boolean;
  menuOpenWidth: number;
  tabStyle: string;
  pageTransition: string;
  customRadius: string;
  darkThemeStyle: "obsidian" | "graphite" | "oceanic" | "nordic";
}
export type AppearanceSaveMode = "server" | "local";
export function useAppearanceSettings() {
  const { t } = useI18n();
  const settingStore = useSettingStore();
  const { switchThemeStyles } = useTheme();
  const {
    configOptions,
    boxStyleOptions,
    tabStyleOptions,
    pageTransitionOptions,
    customRadiusOptions,
  } = useAppearanceSettingsConfig();
  const appearanceLoading = ref(false);
  const appearanceSaving = ref(false);
  const boxStyleOptionsWithIcon = computed(() => boxStyleOptions.value);
  const containerWidthOptions = computed(() => [
    {
      value: ContainerWidthEnum.FULL,
      label: t("setting.container.list.0"),
      icon: "icon-park-outline:auto-width",
    },
    {
      value: ContainerWidthEnum.BOXED,
      label: t("setting.container.list.1"),
      icon: "ri:align-right",
    },
  ]);
  const compactDisplayLabel = (label: string) =>
    label.replace(
      /^(?:\u5f00\u542f|\u663e\u793a|\u542f\u7528|Show |Display |Enable )/,
      "",
    );
  const displaySettings = computed(() => {
    const items = [
      {
        key: "showWorkTab",
        label: compactDisplayLabel(t("systemSettings.display.multiTab")),
      },
      {
        key: "showMenuButton",
        label: compactDisplayLabel(t("systemSettings.display.collapseSidebar")),
      },
      {
        key: "showRefreshButton",
        label: compactDisplayLabel(t("systemSettings.display.reloadPage")),
      },
      {
        key: "showCrumbs",
        label: compactDisplayLabel(t("systemSettings.display.showBreadcrumb")),
      },
      {
        key: "showLanguage",
        label: compactDisplayLabel(t("systemSettings.display.language")),
      },
      {
        key: "showNprogress",
        label: compactDisplayLabel(t("systemSettings.display.showProgressBar")),
      },
      {
        key: "watermarkVisible",
        label: compactDisplayLabel(t("systemSettings.display.watermark")),
      },
    ];
    return items.filter((item) => {
      if (
        item.key === "showMenuButton" &&
        headerBarConfig.menuButton?.enabled === false
      ) {
        return false;
      }
      if (
        item.key === "showRefreshButton" &&
        headerBarConfig.refreshButton?.enabled === false
      ) {
        return false;
      }
      if (
        item.key === "showCrumbs" &&
        headerBarConfig.breadcrumb?.enabled === false
      ) {
        return false;
      }
      if (
        item.key === "showLanguage" &&
        headerBarConfig.language?.enabled === false
      ) {
        return false;
      }
      return true;
    });
  });
  const localAppearanceSettings = reactive<LocalAppearanceSettings>({
    systemThemeMode: SystemThemeEnum.AUTO,
    systemThemeColor: "#5D87FF",
    containerWidth: ContainerWidthEnum.FULL,
    boxBorderMode: false,
    showWorkTab: false,
    showMenuButton: true,
    showRefreshButton: true,
    showCrumbs: true,
    showLanguage: true,
    showNprogress: true,
    watermarkVisible: false,
    menuOpenWidth: 200,
    tabStyle: "tab-default",
    pageTransition: "fade",
    customRadius: "0",
    darkThemeStyle: "obsidian",
  });
  const loadAppearanceSettings = () => {
    localAppearanceSettings.systemThemeMode = settingStore.systemThemeMode;
    localAppearanceSettings.systemThemeColor = settingStore.systemThemeColor;
    localAppearanceSettings.containerWidth = settingStore.containerWidth;
    localAppearanceSettings.boxBorderMode = settingStore.boxBorderMode;
    localAppearanceSettings.showWorkTab = settingStore.showWorkTab;
    localAppearanceSettings.showMenuButton = settingStore.showMenuButton;
    localAppearanceSettings.showRefreshButton = settingStore.showRefreshButton;
    localAppearanceSettings.showCrumbs = settingStore.showCrumbs;
    localAppearanceSettings.showLanguage = settingStore.showLanguage;
    localAppearanceSettings.showNprogress = settingStore.showNprogress;
    localAppearanceSettings.watermarkVisible = settingStore.watermarkVisible;
    localAppearanceSettings.menuOpenWidth = settingStore.menuOpenWidth;
    localAppearanceSettings.tabStyle = settingStore.tabStyle;
    localAppearanceSettings.pageTransition = settingStore.pageTransition;
    localAppearanceSettings.customRadius = settingStore.customRadius;
    localAppearanceSettings.darkThemeStyle = "obsidian";
  };
  const buildAppearanceRequest = (): UpdateAppearanceRequest => ({
    system_theme_mode: localAppearanceSettings.systemThemeMode,
    system_theme_color: localAppearanceSettings.systemThemeColor,
    container_width: localAppearanceSettings.containerWidth,
    box_border_mode: localAppearanceSettings.boxBorderMode,
    show_work_tab: localAppearanceSettings.showWorkTab,
    show_menu_button: localAppearanceSettings.showMenuButton,
    show_refresh_button: localAppearanceSettings.showRefreshButton,
    show_crumbs: localAppearanceSettings.showCrumbs,
    show_language: localAppearanceSettings.showLanguage,
    show_nprogress: localAppearanceSettings.showNprogress,
    watermark_visible: localAppearanceSettings.watermarkVisible,
    menu_open_width: localAppearanceSettings.menuOpenWidth,
    tab_style: localAppearanceSettings.tabStyle,
    page_transition: localAppearanceSettings.pageTransition,
    custom_radius: localAppearanceSettings.customRadius,
    dark_theme_style: localAppearanceSettings.darkThemeStyle,
  });
  const applyAppearanceLocally = () => {
    switchThemeStyles(localAppearanceSettings.systemThemeMode);
    settingStore.setElementTheme(localAppearanceSettings.systemThemeColor);
    settingStore.darkThemeStyle = localAppearanceSettings.darkThemeStyle;
    settingStore.boxBorderMode = localAppearanceSettings.boxBorderMode;
    settingStore.showWorkTab = localAppearanceSettings.showWorkTab;
    settingStore.showMenuButton = localAppearanceSettings.showMenuButton;
    settingStore.showRefreshButton = localAppearanceSettings.showRefreshButton;
    settingStore.showCrumbs = localAppearanceSettings.showCrumbs;
    settingStore.showLanguage = localAppearanceSettings.showLanguage;
    settingStore.showNprogress = localAppearanceSettings.showNprogress;
    settingStore.watermarkVisible = localAppearanceSettings.watermarkVisible;
    settingStore.menuOpenWidth = localAppearanceSettings.menuOpenWidth;
    settingStore.tabStyle = localAppearanceSettings.tabStyle;
    settingStore.pageTransition = localAppearanceSettings.pageTransition;
    settingStore.setCustomRadius(localAppearanceSettings.customRadius);
    settingStore.setContainerWidth(
      localAppearanceSettings.containerWidth as ContainerWidthEnum,
    );
    const boxMode = localAppearanceSettings.boxBorderMode
      ? "border-mode"
      : "shadow-mode";
    document.documentElement.setAttribute("data-box-mode", boxMode);
    settingStore.applyToDom();
  };
  const saveAppearanceSettings = async (options?: {
    silent?: boolean;
  }): Promise<AppearanceSaveMode> => {
    appearanceSaving.value = true;
    try {
      await settingStore.updateServerSettings(buildAppearanceRequest());
      if (
        settingStore.systemThemeColor !==
        localAppearanceSettings.systemThemeColor
      ) {
        settingStore.setElementTheme(localAppearanceSettings.systemThemeColor);
      }
      if (!options?.silent) {
        ElMessage.success(t("systemSettings.messages.appearanceSaved"));
      }
      return "server";
    } catch (error) {
      logger.error("Server save failed, applying locally");
      applyAppearanceLocally();
      if (!options?.silent) {
        ElMessage.warning(t("systemSettings.messages.appearanceSavedLocally"));
      }
      return "local";
    } finally {
      appearanceSaving.value = false;
    }
  };
  const handleAppearanceSave = async () => {
    await saveAppearanceSettings();
  };
  const handleAppearanceReset = async () => {
    try {
      await ElMessageBox.confirm(
        t("systemSettings.messages.resetConfirmMessage"),
        t("systemSettings.messages.resetConfirmTitle"),
        {
          confirmButtonText: t("common.confirm"),
          cancelButtonText: t("common.cancel"),
          type: "warning",
        },
      );
      appearanceSaving.value = true;
      try {
        await settingStore.resetServerSettings();
      } catch {
        localStorage.removeItem("setting");
      }
      loadAppearanceSettings();
      ElMessage.success(t("systemSettings.messages.appearanceResetSuccess"));
    } catch (error) {
      if (error !== "cancel") {
        logger.error("Failed to reset appearance settings");
      }
    } finally {
      appearanceSaving.value = false;
    }
  };
  watch(
    () => localAppearanceSettings.customRadius,
    (newRadius) => {
      settingStore.setCustomRadius(newRadius);
    },
  );
  watch(
    () => localAppearanceSettings.systemThemeMode,
    (newThemeMode) => {
      switchThemeStyles(newThemeMode);
    },
  );
  watch(
    () => localAppearanceSettings.darkThemeStyle,
    (newStyle) => {
      settingStore.darkThemeStyle = newStyle;
      settingStore.applyToDom();
    },
  );
  watch(
    () => settingStore.systemThemeMode,
    (newThemeMode) => {
      if (localAppearanceSettings.systemThemeMode !== newThemeMode) {
        localAppearanceSettings.systemThemeMode = newThemeMode;
      }
    },
  );
  watch(
    () => settingStore.systemThemeColor,
    (newColor) => {
      if (localAppearanceSettings.systemThemeColor !== newColor) {
        localAppearanceSettings.systemThemeColor = newColor;
      }
    },
  );
  watch(
    () => settingStore.darkThemeStyle,
    (newStyle) => {
      if (localAppearanceSettings.darkThemeStyle !== newStyle) {
        localAppearanceSettings.darkThemeStyle = newStyle as
          | "obsidian"
          | "graphite"
          | "oceanic"
          | "nordic";
      }
    },
  );
  return {
    configOptions,
    tabStyleOptions,
    pageTransitionOptions,
    customRadiusOptions,
    boxStyleOptionsWithIcon,
    containerWidthOptions,
    displaySettings,
    localAppearanceSettings,
    appearanceLoading,
    appearanceSaving,
    loadAppearanceSettings,
    saveAppearanceSettings,
    handleAppearanceSave,
    handleAppearanceReset,
  };
}
