import { computed } from "vue";
import { storeToRefs } from "pinia";
import { useSettingStore } from "@/store/modules/setting";
import { headerBarConfig } from "@/config/modules/headerBar";
import { HeaderBarFeatureConfig } from "@/types";
export function useHeaderBar() {
  const settingStore = useSettingStore();
  const headerBarConfigRef = computed<HeaderBarFeatureConfig>(
    () => headerBarConfig,
  );
  const {
    showMenuButton,
    showFastEnter,
    showRefreshButton,
    showCrumbs,
    showLanguage,
  } = storeToRefs(settingStore);
  const isFeatureEnabled = (feature: keyof HeaderBarFeatureConfig): boolean => {
    return headerBarConfigRef.value[feature]?.enabled ?? false;
  };
  const getFeatureConfig = (feature: keyof HeaderBarFeatureConfig) => {
    return headerBarConfigRef.value[feature];
  };
  const shouldShowMenuButton = computed(() => {
    return isFeatureEnabled("menuButton") && showMenuButton.value;
  });
  const shouldShowRefreshButton = computed(() => {
    return isFeatureEnabled("refreshButton") && showRefreshButton.value;
  });
  const shouldShowFastEnter = computed(() => {
    return isFeatureEnabled("fastEnter") && showFastEnter.value;
  });
  const shouldShowBreadcrumb = computed(() => {
    return isFeatureEnabled("breadcrumb") && showCrumbs.value;
  });
  const shouldShowFullscreen = computed(() => {
    return isFeatureEnabled("fullscreen");
  });
  const shouldShowLanguage = computed(() => {
    return isFeatureEnabled("language") && showLanguage.value;
  });
  const shouldShowThemeToggle = computed(() => {
    return isFeatureEnabled("themeToggle");
  });
  const fastEnterMinWidth = computed(() => {
    const config = getFeatureConfig("fastEnter");
    return (config as any)?.minWidth || 1200;
  });
  const isFeatureActive = (feature: keyof HeaderBarFeatureConfig): boolean => {
    return isFeatureEnabled(feature);
  };
  const getFeatureInfo = (feature: keyof HeaderBarFeatureConfig) => {
    return getFeatureConfig(feature);
  };
  const getEnabledFeatures = (): (keyof HeaderBarFeatureConfig)[] => {
    return Object.keys(headerBarConfigRef.value).filter(
      (key) =>
        headerBarConfigRef.value[key as keyof HeaderBarFeatureConfig]?.enabled,
    ) as (keyof HeaderBarFeatureConfig)[];
  };
  const getDisabledFeatures = (): (keyof HeaderBarFeatureConfig)[] => {
    return Object.keys(headerBarConfigRef.value).filter(
      (key) =>
        !headerBarConfigRef.value[key as keyof HeaderBarFeatureConfig]?.enabled,
    ) as (keyof HeaderBarFeatureConfig)[];
  };
  const getActiveFeatures = () => {
    return getEnabledFeatures();
  };
  const getInactiveFeatures = () => {
    return getDisabledFeatures();
  };
  return {
    headerBarConfig: headerBarConfigRef,
    shouldShowMenuButton,
    shouldShowRefreshButton,
    shouldShowFastEnter,
    shouldShowBreadcrumb,
    shouldShowFullscreen,
    shouldShowLanguage,
    shouldShowThemeToggle,
    fastEnterMinWidth,
    isFeatureEnabled,
    isFeatureActive,
    getFeatureConfig,
    getFeatureInfo,
    getEnabledFeatures,
    getDisabledFeatures,
    getActiveFeatures,
    getInactiveFeatures,
  };
}
