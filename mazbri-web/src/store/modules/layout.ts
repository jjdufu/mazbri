import { defineStore } from "pinia";
import { ref, computed } from "vue";
import { MenuTypeEnum } from "@/enums/appEnum";
import { SETTING_DEFAULT_CONFIG } from "@/config/setting";
export const useLayoutStore = defineStore("layoutStore", () => {
  const menuType = ref(SETTING_DEFAULT_CONFIG.menuType);
  const menuOpenWidth = ref(SETTING_DEFAULT_CONFIG.menuOpenWidth);
  const menuOpen = ref(SETTING_DEFAULT_CONFIG.menuOpen);
  const dualMenuShowText = ref(SETTING_DEFAULT_CONFIG.dualMenuShowText);
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
  const setBorderMode = () => {
    boxBorderMode.value = !boxBorderMode.value;
  };
  const applyLayoutToDom = () => {
    document.documentElement.style.removeProperty("--container-width");
    const boxMode = boxBorderMode.value ? "border-mode" : "shadow-mode";
    document.documentElement.setAttribute("data-box-mode", boxMode);
  };
  return {
    menuType,
    menuOpenWidth,
    menuOpen,
    dualMenuShowText,
    boxBorderMode,
    getMenuOpenWidth,
    switchMenuLayouts,
    setMenuOpenWidth,
    setMenuOpen,
    setDualMenuShowText,
    setBorderMode,
    applyLayoutToDom,
  };
});
