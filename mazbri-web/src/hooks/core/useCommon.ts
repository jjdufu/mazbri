import { computed } from "vue";
import { useMenuStore } from "@/store/modules/menu";
import { useSettingStore } from "@/store/modules/setting";
export function useCommon() {
  const menuStore = useMenuStore();
  const settingStore = useSettingStore();
  const homePath = computed(() => menuStore.getHomePath());
  const refresh = () => {
    settingStore.reload();
  };
  const scrollToTop = () => {
    const scrollContainer = document.getElementById("app-main");
    if (scrollContainer) {
      scrollContainer.scrollTop = 0;
    }
  };
  const smoothScrollToTop = () => {
    const scrollContainer = document.getElementById("app-main");
    if (scrollContainer) {
      scrollContainer.scrollTo({
        top: 0,
        behavior: "smooth",
      });
    }
  };
  const scrollTo = (top: number, smooth: boolean = false) => {
    const scrollContainer = document.getElementById("app-main");
    if (scrollContainer) {
      scrollContainer.scrollTo({
        top,
        behavior: smooth ? "smooth" : "auto",
      });
    }
  };
  return {
    homePath,
    refresh,
    scrollTo,
    scrollToTop,
    smoothScrollToTop,
  };
}
