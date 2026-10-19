import { defineStore } from "pinia";
import { ref, computed } from "vue";
import { SETTING_DEFAULT_CONFIG } from "@/config/setting";
export const useDisplayStore = defineStore("displayStore", () => {
  const showMenuButton = ref(SETTING_DEFAULT_CONFIG.showMenuButton);
  const showFastEnter = ref(SETTING_DEFAULT_CONFIG.showFastEnter);
  const showRefreshButton = ref(SETTING_DEFAULT_CONFIG.showRefreshButton);
  const showCrumbs = ref(SETTING_DEFAULT_CONFIG.showCrumbs);
  const showWorkTab = ref(SETTING_DEFAULT_CONFIG.showWorkTab);
  const showLanguage = ref(SETTING_DEFAULT_CONFIG.showLanguage);
  const showNprogress = ref(SETTING_DEFAULT_CONFIG.showNprogress);
  const showFestivalText = ref(SETTING_DEFAULT_CONFIG.showFestivalText);
  const watermarkVisible = ref(SETTING_DEFAULT_CONFIG.watermarkVisible);
  const autoClose = ref(SETTING_DEFAULT_CONFIG.autoClose);
  const holidayFireworksLoaded = ref(
    SETTING_DEFAULT_CONFIG.holidayFireworksLoaded,
  );
  const pageTransition = ref(SETTING_DEFAULT_CONFIG.pageTransition);
  const tabStyle = ref(SETTING_DEFAULT_CONFIG.tabStyle);
  const customRadius = ref(SETTING_DEFAULT_CONFIG.customRadius);
  const festivalDate = ref(SETTING_DEFAULT_CONFIG.festivalDate);
  const refresh = ref(SETTING_DEFAULT_CONFIG.refresh);
  const getCustomRadius = computed((): string => {
    return (customRadius.value || SETTING_DEFAULT_CONFIG.customRadius) + "rem";
  });
  const setButton = () => {
    showMenuButton.value = !showMenuButton.value;
  };
  const setFastEnter = () => {
    showFastEnter.value = !showFastEnter.value;
  };
  const setAutoClose = () => {
    autoClose.value = !autoClose.value;
  };
  const setShowRefreshButton = () => {
    showRefreshButton.value = !showRefreshButton.value;
  };
  const setCrumbs = () => {
    showCrumbs.value = !showCrumbs.value;
  };
  const setWorkTab = (show: boolean) => {
    showWorkTab.value = show;
  };
  const setLanguage = () => {
    showLanguage.value = !showLanguage.value;
  };
  const setNprogress = () => {
    showNprogress.value = !showNprogress.value;
  };
  const setPageTransition = (transition: string) => {
    pageTransition.value = transition;
  };
  const setTabStyle = (style: string) => {
    tabStyle.value = style;
  };
  const setWatermarkVisible = (visible: boolean) => {
    watermarkVisible.value = visible;
  };
  const setCustomRadius = (radius: string) => {
    customRadius.value = radius;
    applyRadiusToDom();
  };
  const setholidayFireworksLoaded = (isLoad: boolean) => {
    holidayFireworksLoaded.value = isLoad;
  };
  const setShowFestivalText = (show: boolean) => {
    showFestivalText.value = show;
  };
  const setFestivalDate = (date: string) => {
    festivalDate.value = date;
  };
  const reload = () => {
    refresh.value = !refresh.value;
  };
  const applyRadiusToDom = () => {
    const radiusRem = `${customRadius.value}rem`;
    const isZero = parseFloat(customRadius.value) === 0;
    if (isZero) {
      document.documentElement.classList.add("radius-zero");
    } else {
      document.documentElement.classList.remove("radius-zero");
    }
    document.documentElement.style.setProperty("--custom-radius", radiusRem);
    document.documentElement.style.setProperty(
      "--el-border-radius-base",
      `calc(${radiusRem} / 3 + 2px)`,
    );
    document.documentElement.style.setProperty(
      "--el-border-radius-small",
      `calc(${radiusRem} / 3 + 4px)`,
    );
    document.documentElement.style.setProperty(
      "--el-messagebox-border-radius",
      `calc(${radiusRem} / 3 + 4px)`,
    );
    document.documentElement.style.setProperty(
      "--el-popover-border-radius",
      `calc(${radiusRem} / 3 + 4px)`,
    );
    document.documentElement.style.setProperty(
      "--el-card-border-radius",
      `calc(${radiusRem} + 4px)`,
    );
  };
  return {
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
    setButton,
    setFastEnter,
    setAutoClose,
    setShowRefreshButton,
    setCrumbs,
    setWorkTab,
    setLanguage,
    setNprogress,
    setPageTransition,
    setTabStyle,
    setWatermarkVisible,
    setCustomRadius,
    setholidayFireworksLoaded,
    setShowFestivalText,
    setFestivalDate,
    reload,
    applyRadiusToDom,
  };
});
