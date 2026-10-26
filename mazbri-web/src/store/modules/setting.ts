import { defineStore } from "pinia";
import { computed, ref } from "vue";
import AppConfig from "@/config";
import { ContainerWidthEnum, SystemThemeEnum } from "@/enums/appEnum";
import { useCeremony } from "@/hooks/core/useCeremony";
import type {
  AppearanceSettings,
  UpdateAppearanceRequest,
} from "@/types/api/appearance";
import type { SystemSettings } from "@/types/api/settings";
import { logger } from "@/utils/sys";
import { formatMenuTitle } from "@/utils/router";
import { router } from "@/router";
import { fetchSystemSettings } from "@/api/settings";
import { useThemeStore } from "./theme";
import { useLayoutStore } from "./layout";
import { useDisplayStore } from "./display";
/**
 * Bumped every time server appearance settings are applied. Module-level (not
 * store state) so it is never persisted to localStorage.
 */
export const appearanceServerRevision = ref(0);
export const useSettingStore = defineStore(
  "settingStore",
  () => {
    const themeStore = useThemeStore();
    const layoutStore = useLayoutStore();
    const displayStore = useDisplayStore();
    const siteName = ref(AppConfig.systemInfo.name);
    const siteUrl = ref(AppConfig.systemInfo.siteUrl || "");

    const tenantGatewayUrl = ref("");
    const serverLoaded = computed(() => false);
    const serverLoading = computed(() => false);
    const systemThemeType = computed({
      get: () => themeStore.systemThemeType,
      set: (value) => {
        themeStore.systemThemeType = value;
      },
    });
    const systemThemeMode = computed({
      get: () => themeStore.systemThemeMode,
      set: (value) => {
        themeStore.systemThemeMode = value;
      },
    });
    const menuThemeType = computed({
      get: () => themeStore.menuThemeType,
      set: (value) => {
        themeStore.menuThemeType = value;
      },
    });
    const systemThemeColor = computed({
      get: () => themeStore.systemThemeColor,
      set: (value) => {
        themeStore.systemThemeColor = value;
      },
    });
    const darkThemeStyle = computed({
      get: () => themeStore.darkThemeStyle,
      set: (value) => {
        themeStore.darkThemeStyle = value;
      },
    });
    const isDark = computed(() => themeStore.isDark);
    const getMenuTheme = computed(() => themeStore.getMenuTheme);
    const menuType = computed(() => layoutStore.menuType);
    const menuOpenWidth = computed({
      get: () => layoutStore.menuOpenWidth,
      set: (value) => {
        layoutStore.menuOpenWidth = value;
      },
    });
    const menuOpen = computed(() => layoutStore.menuOpen);
    const dualMenuShowText = computed(() => layoutStore.dualMenuShowText);
    const containerWidth = computed({
      get: () => layoutStore.containerWidth,
      set: (value) => {
        layoutStore.containerWidth = value;
      },
    });
    const boxBorderMode = computed({
      get: () => layoutStore.boxBorderMode,
      set: (value) => {
        layoutStore.boxBorderMode = value;
      },
    });
    const getMenuOpenWidth = computed(() => layoutStore.getMenuOpenWidth);
    const showMenuButton = computed({
      get: () => displayStore.showMenuButton,
      set: (value) => {
        displayStore.showMenuButton = value;
      },
    });
    const showFastEnter = computed({
      get: () => displayStore.showFastEnter,
      set: (value) => {
        displayStore.showFastEnter = value;
      },
    });
    const showRefreshButton = computed({
      get: () => displayStore.showRefreshButton,
      set: (value) => {
        displayStore.showRefreshButton = value;
      },
    });
    const showCrumbs = computed({
      get: () => displayStore.showCrumbs,
      set: (value) => {
        displayStore.showCrumbs = value;
      },
    });
    const showWorkTab = computed({
      get: () => displayStore.showWorkTab,
      set: (value) => {
        displayStore.showWorkTab = value;
      },
    });
    const showLanguage = computed({
      get: () => displayStore.showLanguage,
      set: (value) => {
        displayStore.showLanguage = value;
      },
    });
    const showNprogress = computed({
      get: () => displayStore.showNprogress,
      set: (value) => {
        displayStore.showNprogress = value;
      },
    });
    const showFestivalText = computed(() => displayStore.showFestivalText);
    const watermarkVisible = computed({
      get: () => displayStore.watermarkVisible,
      set: (value) => {
        displayStore.watermarkVisible = value;
      },
    });
    const autoClose = computed(() => displayStore.autoClose);
    const holidayFireworksLoaded = computed(
      () => displayStore.holidayFireworksLoaded,
    );
    const pageTransition = computed({
      get: () => displayStore.pageTransition,
      set: (value) => {
        displayStore.pageTransition = value;
      },
    });
    const tabStyle = computed({
      get: () => displayStore.tabStyle,
      set: (value) => {
        displayStore.tabStyle = value;
      },
    });
    const customRadius = computed({
      get: () => displayStore.customRadius,
      set: (value) => {
        displayStore.customRadius = value;
      },
    });
    const festivalDate = computed(() => displayStore.festivalDate);
    const refresh = computed(() => displayStore.refresh);
    const getCustomRadius = computed(() => displayStore.getCustomRadius);
    const isShowFireworks = computed((): boolean => {
      return festivalDate.value ===
        useCeremony().currentFestivalData.value?.date
        ? false
        : true;
    });
    const applySystemSettings = (settings: SystemSettings) => {
      const nextSiteName =
        settings.site_name?.trim() || AppConfig.systemInfo.name;
      const nextSiteUrl = settings.site_url?.trim() || "";
      const nextGateway = settings.tenant_gateway_url?.trim() || "";
      siteName.value = nextSiteName;
      siteUrl.value = nextSiteUrl;
      tenantGatewayUrl.value = nextGateway.replace(/\/+$/, "");
      AppConfig.systemInfo.name = nextSiteName;
      AppConfig.systemInfo.siteUrl = nextSiteUrl;
      const currentTitle = router.currentRoute.value.meta.title;
      if (currentTitle) {

        document.title = `${formatMenuTitle(String(currentTitle))} - ${nextSiteName}`;
      }
    };
    const applyServerSettings = (settings: AppearanceSettings) => {
      const systemThemeMode =
        settings.system_theme_mode || SystemThemeEnum.AUTO;
      themeStore.systemThemeMode = systemThemeMode;
      if (systemThemeMode !== SystemThemeEnum.AUTO) {
        themeStore.systemThemeType = systemThemeMode;
      }
      themeStore.systemThemeColor = settings.system_theme_color;
      themeStore.darkThemeStyle = settings.dark_theme_style || "obsidian";
      layoutStore.containerWidth =
        settings.container_width as ContainerWidthEnum;
      layoutStore.boxBorderMode = settings.box_border_mode;
      layoutStore.menuOpenWidth = settings.menu_open_width;
      displayStore.showWorkTab = settings.show_work_tab;
      displayStore.showMenuButton = settings.show_menu_button;
      displayStore.showFastEnter = settings.show_fast_enter;
      displayStore.showRefreshButton = settings.show_refresh_button;
      displayStore.showCrumbs = settings.show_crumbs;
      displayStore.showLanguage = settings.show_language;
      displayStore.showNprogress = settings.show_nprogress;
      displayStore.watermarkVisible = settings.watermark_visible;
      displayStore.tabStyle = settings.tab_style || displayStore.tabStyle;
      displayStore.pageTransition =
        settings.page_transition || displayStore.pageTransition;
      displayStore.customRadius =
        settings.custom_radius || displayStore.customRadius;
      applyToDom();
      appearanceServerRevision.value += 1;
    };
    const applyToDom = () => {
      themeStore.applyThemeToDom();
      layoutStore.applyLayoutToDom();
      displayStore.applyRadiusToDom();
    };
    const loadFromServer = async () => {
      await Promise.allSettled([
        loadAppearanceSettings(),
        loadSystemSettings(),
      ]);
    };
    const loadAppearanceSettings = async () => {
      try {
        const { fetchAppearanceSettings } = await import("@/api/appearance");
        const settings = await fetchAppearanceSettings();
        applyServerSettings(settings);
        return settings;
      } catch (error) {
        logger.error("Failed to load appearance settings from server");
        throw error;
      }
    };
    const loadSystemSettings = async () => {
      try {
        const settings = await fetchSystemSettings();
        applySystemSettings(settings);
        return settings;
      } catch (error) {
        logger.error("Failed to load system settings from server");
        throw error;
      }
    };
    const updateServerSettings = async (data: UpdateAppearanceRequest) => {
      const { updateAppearanceSettings } = await import("@/api/appearance");
      const settings = await updateAppearanceSettings(data);
      applyServerSettings(settings);
      return settings;
    };
    const resetServerSettings = async () => {
      const { resetAppearanceSettings } = await import("@/api/appearance");
      const settings = await resetAppearanceSettings();
      applyServerSettings(settings);
      return settings;
    };
    return {
      themeStore,
      layoutStore,
      displayStore,
      serverLoaded,
      serverLoading,
      systemThemeType,
      systemThemeMode,
      menuThemeType,
      systemThemeColor,
      darkThemeStyle,
      isDark,
      getMenuTheme,
      menuType,
      menuOpenWidth,
      menuOpen,
      dualMenuShowText,
      containerWidth,
      boxBorderMode,
      getMenuOpenWidth,
      showMenuButton,
      showFastEnter,
      showRefreshButton,
      showCrumbs,
      showWorkTab,
      showLanguage,
      showNprogress,
      showFestivalText,
      watermarkVisible,
      autoClose,
      holidayFireworksLoaded,
      pageTransition,
      tabStyle,
      customRadius,
      festivalDate,
      refresh,
      getCustomRadius,
      isShowFireworks,
      siteName,
      siteUrl,
      tenantGatewayUrl,
      loadFromServer,
      loadAppearanceSettings,
      loadSystemSettings,
      updateServerSettings,
      resetServerSettings,
      applyToDom,
      applySystemSettings,
      setGlopTheme: themeStore.setGlopTheme,
      switchMenuStyles: themeStore.switchMenuStyles,
      setElementTheme: themeStore.setElementTheme,
      switchMenuLayouts: layoutStore.switchMenuLayouts,
      setMenuOpenWidth: layoutStore.setMenuOpenWidth,
      setMenuOpen: layoutStore.setMenuOpen,
      setDualMenuShowText: layoutStore.setDualMenuShowText,
      setContainerWidth: layoutStore.setContainerWidth,
      setBorderMode: layoutStore.setBorderMode,
      setButton: displayStore.setButton,
      setFastEnter: displayStore.setFastEnter,
      setAutoClose: displayStore.setAutoClose,
      setShowRefreshButton: displayStore.setShowRefreshButton,
      setCrumbs: displayStore.setCrumbs,
      setWorkTab: displayStore.setWorkTab,
      setLanguage: displayStore.setLanguage,
      setNprogress: displayStore.setNprogress,
      setPageTransition: displayStore.setPageTransition,
      setTabStyle: displayStore.setTabStyle,
      setWatermarkVisible: displayStore.setWatermarkVisible,
      setCustomRadius: displayStore.setCustomRadius,
      setholidayFireworksLoaded: displayStore.setholidayFireworksLoaded,
      setShowFestivalText: displayStore.setShowFestivalText,
      setFestivalDate: displayStore.setFestivalDate,
      reload: displayStore.reload,
    };
  },
  {
    persist: {
      key: "setting",
      storage: localStorage,
    },
  },
);
