import { defineStore } from "pinia";
import { computed, ref } from "vue";
import AppConfig from "@/config";
import { useCeremony } from "@/hooks/core/useCeremony";
import type { SystemSettings } from "@/types/api/settings";
import { logger } from "@/utils/sys";
import { formatMenuTitle } from "@/utils/router";
import { router } from "@/router";
import { fetchSystemSettings } from "@/api/settings";
import { useThemeStore } from "./theme";
import { useLayoutStore } from "./layout";
import { useDisplayStore } from "./display";

import { StorageConfig } from "@/utils";

if (typeof localStorage !== "undefined") {
  localStorage.removeItem("setting");
  const settingKeyPattern = StorageConfig.createKeyPattern("setting");
  for (const key of Object.keys(localStorage)) {
    if (settingKeyPattern.test(key)) {
      localStorage.removeItem(key);
    }
  }
}

const serverLoadedFlag = ref(false);
const serverLoadingFlag = ref(false);
let loadInflight: Promise<void> | null = null;

export const useSettingStore = defineStore("settingStore", () => {
  const themeStore = useThemeStore();
  const layoutStore = useLayoutStore();
  const displayStore = useDisplayStore();
  const siteName = ref(AppConfig.systemInfo.name);
  const siteUrl = ref(AppConfig.systemInfo.siteUrl || "");
  const tenantGatewayUrl = ref("");
  const serverLoaded = computed(() => serverLoadedFlag.value);
  const serverLoading = computed(() => serverLoadingFlag.value);

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
  const menuOpenWidth = computed(() => layoutStore.menuOpenWidth);
  const menuOpen = computed(() => layoutStore.menuOpen);
  const dualMenuShowText = computed(() => layoutStore.dualMenuShowText);
  const boxBorderMode = computed(() => layoutStore.boxBorderMode);
  const getMenuOpenWidth = computed(() => layoutStore.getMenuOpenWidth);
  const showMenuButton = computed(() => displayStore.showMenuButton);
  const showFastEnter = computed(() => displayStore.showFastEnter);
  const showRefreshButton = computed(() => displayStore.showRefreshButton);
  const showCrumbs = computed(() => displayStore.showCrumbs);
  const showWorkTab = computed(() => displayStore.showWorkTab);
  const showLanguage = computed(() => displayStore.showLanguage);
  const showNprogress = computed(() => displayStore.showNprogress);
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
  const pageTransition = computed(() => displayStore.pageTransition);
  const tabStyle = computed(() => displayStore.tabStyle);
  const customRadius = computed(() => displayStore.customRadius);
  const festivalDate = computed(() => displayStore.festivalDate);
  const refresh = computed(() => displayStore.refresh);
  const getCustomRadius = computed(() => displayStore.getCustomRadius);
  const isShowFireworks = computed((): boolean => {
    return festivalDate.value === useCeremony().currentFestivalData.value?.date
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
    displayStore.watermarkVisible = Boolean(settings.watermark_visible);
    AppConfig.systemInfo.name = nextSiteName;
    AppConfig.systemInfo.siteUrl = nextSiteUrl;
    const currentTitle = router.currentRoute.value.meta.title;
    if (currentTitle) {
      document.title = `${formatMenuTitle(String(currentTitle))} - ${nextSiteName}`;
    }
  };

  const applyToDom = () => {
    themeStore.applyThemeToDom();
    layoutStore.applyLayoutToDom();
    displayStore.applyRadiusToDom();
  };

  const loadSystemSettings = async (force = false) => {
    try {
      const settings = await fetchSystemSettings(force);
      applySystemSettings(settings);
      return settings;
    } catch (error) {
      logger.error("Failed to load system settings from server");
      throw error;
    }
  };

  const loadFromServer = async (force = false) => {
    applyToDom();
    if (!force && serverLoadedFlag.value) return;
    if (!force && loadInflight) return loadInflight;
    serverLoadingFlag.value = true;
    loadInflight = (async () => {
      try {
        await loadSystemSettings(force);
        serverLoadedFlag.value = true;
      } finally {
        serverLoadingFlag.value = false;
        loadInflight = null;
      }
    })();
    return loadInflight;
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
    loadSystemSettings,
    applyToDom,
    applySystemSettings,
    setGlopTheme: themeStore.setGlopTheme,
    switchMenuStyles: themeStore.switchMenuStyles,
    setElementTheme: themeStore.setElementTheme,
    switchMenuLayouts: layoutStore.switchMenuLayouts,
    setMenuOpenWidth: layoutStore.setMenuOpenWidth,
    setMenuOpen: layoutStore.setMenuOpen,
    setDualMenuShowText: layoutStore.setDualMenuShowText,
    setButton: displayStore.setButton,
    setFastEnter: displayStore.setFastEnter,
    setAutoClose: displayStore.setAutoClose,
    setShowRefreshButton: displayStore.setShowRefreshButton,
    setCrumbs: displayStore.setCrumbs,
    setLanguage: displayStore.setLanguage,
    setNprogress: displayStore.setNprogress,
    setWatermarkVisible: displayStore.setWatermarkVisible,
    setholidayFireworksLoaded: displayStore.setholidayFireworksLoaded,
    setShowFestivalText: displayStore.setShowFestivalText,
    setFestivalDate: displayStore.setFestivalDate,
    reload: displayStore.reload,
  };
});
