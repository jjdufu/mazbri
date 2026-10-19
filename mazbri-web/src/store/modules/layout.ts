import { defineStore } from "pinia";
import { ref, computed } from "vue";
import { MenuTypeEnum, ContainerWidthEnum } from "@/enums/appEnum";
import { SETTING_DEFAULT_CONFIG } from "@/config/setting";
export const useLayoutStore = defineStore("layoutStore", () => {
  const menuType = ref(SETTING_DEFAULT_CONFIG.menuType);
  const menuOpenWidth = ref(SETTING_DEFAULT_CONFIG.menuOpenWidth);
  const menuOpen = ref(SETTING_DEFAULT_CONFIG.menuOpen);
  const dualMenuShowText = ref(SETTING_DEFAULT_CONFIG.dualMenuShowText);
  const containerWidth = ref(SETTING_DEFAULT_CONFIG.containerWidth);
  const boxBorderMode = ref(SETTING_DEFAULT_CONFIG.boxBorderMode);
  const getMenuOpenWidth = computed((): string => {
    return (menuOpenWidth.value || SETTING_DEFAULT_CONFIG.menuOpenWidth) + "px";
  });
  const switchMenuLayouts = (type: MenuTypeEnum) => {
    menuType.value = type;
  };
  const setMenuOpenWidth = (width: number) => {
    menuOpenWidth.value = width;
  };
  const setMenuOpen = (open: boolean) => {
    menuOpen.value = open;
  };
  const setDualMenuShowText = (show: boolean) => {
    dualMenuShowText.value = show;
  };
  const setContainerWidth = (width: ContainerWidthEnum) => {
    containerWidth.value = width;
  };
  const setBorderMode = () => {
    boxBorderMode.value = !boxBorderMode.value;
  };
  const applyLayoutToDom = () => {
    document.documentElement.style.setProperty(
      "--container-width",
      containerWidth.value,
    );
    const boxMode = boxBorderMode.value ? "border-mode" : "shadow-mode";
    document.documentElement.setAttribute("data-box-mode", boxMode);
  };
  return {
    menuType,
    menuOpenWidth,
    menuOpen,
    dualMenuShowText,
    containerWidth,
    boxBorderMode,
    getMenuOpenWidth,
    switchMenuLayouts,
    setMenuOpenWidth,
    setMenuOpen,
    setDualMenuShowText,
    setContainerWidth,
    setBorderMode,
    applyLayoutToDom,
  };
});
