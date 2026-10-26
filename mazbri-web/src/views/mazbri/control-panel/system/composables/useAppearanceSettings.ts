import {
  ref,
  reactive,
  computed,
  watch,
  onBeforeUnmount,
  onDeactivated,
  onActivated,
  onMounted,
} from "vue";
import { useI18n } from "vue-i18n";
import {
  useSettingStore,
  appearanceServerRevision,
} from "@/store/modules/setting";
import { ContainerWidthEnum, SystemThemeEnum } from "@/enums/appEnum";
import { headerBarConfig } from "@/config/modules/headerBar";
import { ElMessage, ElMessageBox } from "element-plus";
import { logger } from "@/utils/sys";
import type { UpdateAppearanceRequest } from "@/types/api/appearance";
import { useTheme } from "@/hooks/core/useTheme";
import { useAppearanceSettingsConfig } from "./useAppearanceSettingsConfig";
import { createAppearancePreview } from "./appearancePreview";
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
  const readStoreAppearance = (): LocalAppearanceSettings => ({
    systemThemeMode: settingStore.systemThemeMode,
    systemThemeColor: settingStore.systemThemeColor,
    containerWidth: settingStore.containerWidth,
    boxBorderMode: settingStore.boxBorderMode,
    showWorkTab: settingStore.showWorkTab,
    showMenuButton: settingStore.showMenuButton,
    showRefreshButton: settingStore.showRefreshButton,
    showCrumbs: settingStore.showCrumbs,
    showLanguage: settingStore.showLanguage,
    showNprogress: settingStore.showNprogress,
    watermarkVisible: settingStore.watermarkVisible,
    menuOpenWidth: settingStore.menuOpenWidth,
    tabStyle: settingStore.tabStyle,
    pageTransition: settingStore.pageTransition,
    customRadius: settingStore.customRadius,
    darkThemeStyle:
      settingStore.darkThemeStyle as LocalAppearanceSettings["darkThemeStyle"],
  });
  // Applies values to the in-memory stores + DOM only (preview). The appearance
  // stores are not persisted, so nothing here reaches the server or survives a
  // reload; App.vue reloads the server values on startup.
  const applyAppearanceValues = (values: LocalAppearanceSettings) => {
    switchThemeStyles(values.systemThemeMode);
    settingStore.setElementTheme(values.systemThemeColor);
    settingStore.darkThemeStyle = values.darkThemeStyle;
    settingStore.boxBorderMode = values.boxBorderMode;
    settingStore.showWorkTab = values.showWorkTab;
    settingStore.showMenuButton = values.showMenuButton;
    settingStore.showRefreshButton = values.showRefreshButton;
    settingStore.showCrumbs = values.showCrumbs;
    settingStore.showLanguage = values.showLanguage;
    settingStore.showNprogress = values.showNprogress;
    settingStore.watermarkVisible = values.watermarkVisible;
    settingStore.menuOpenWidth = values.menuOpenWidth;
    settingStore.tabStyle = values.tabStyle;
    settingStore.pageTransition = values.pageTransition;
    settingStore.setCustomRadius(values.customRadius);
    settingStore.setContainerWidth(values.containerWidth as ContainerWidthEnum);
    const boxMode = values.boxBorderMode ? "border-mode" : "shadow-mode";
    document.documentElement.setAttribute("data-box-mode", boxMode);
    settingStore.applyToDom();
  };
  let pageActive = false;
  const appearancePreview = createAppearancePreview(localAppearanceSettings, {
    read: readStoreAppearance,
    apply: applyAppearanceValues,
    isActive: () => pageActive,
  });
  const loadAppearanceSettings = () => {
    const current = readStoreAppearance();
    Object.assign(localAppearanceSettings, current);
    localAppearanceSettings.darkThemeStyle = "obsidian";
    appearancePreview.commit();
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
    applyAppearanceValues({ ...localAppearanceSettings });
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
      appearancePreview.commit();
      if (!options?.silent) {
        ElMessage.success(t("systemSettings.messages.appearanceSaved"));
      }
      return "server";
    } catch (error) {
      logger.error("Server save failed, applying locally");
      applyAppearanceLocally();
      // Kept for this session only (existing "saved locally" semantics).
      appearancePreview.commit();
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
        appearancePreview.revert();
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
  // Re-sync local values + baseline whenever server settings are applied
  // (initial load finishing late, saves from elsewhere such as the header
  // theme toggle, save/reset on this page).
  watch(appearanceServerRevision, () => {
    if (pageActive) {
      loadAppearanceSettings();
    }
  });
  const activate = () => {
    pageActive = true;
  };
  const deactivate = () => {
    if (!pageActive) {
      return;
    }
    // Leaving without saving: drop the live preview.
    appearancePreview.revert();
    pageActive = false;
  };
  onMounted(activate);
  onActivated(activate);
  onDeactivated(deactivate);
  onBeforeUnmount(deactivate);
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
