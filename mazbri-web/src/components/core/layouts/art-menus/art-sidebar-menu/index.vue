<template>
  <div
    v-if="showLeftMenu || isDualMenu"
    class="layout-sidebar"
    :class="{ 'no-border': menuList.length === 0 }"
  >
    <div
      v-if="isDualMenu"
      class="dual-menu-left"
      :style="{
        width: dualMenuShowText ? '80px' : '64px',
        background: getMenuTheme.background,
      }"
    >
      <ArtLogo class="logo" @click="navigateToHome" />

      <ElScrollbar style="height: calc(100% - 135px)">
        <ul>
          <li
            v-for="menu in firstLevelMenus"
            :key="menu.path"
            @click="handleMenuJump(menu, true)"
          >
            <ElTooltip
              class="box-item"
              effect="dark"
              :content="$t(menu.meta.title)"
              placement="right"
              :offset="15"
              :hide-after="0"
              :disabled="dualMenuShowText"
            >
              <div
                :class="{
                  'is-active': menu.meta.isFirstLevel
                    ? menu.path === route.path
                    : menu.path === firstLevelMenuPath,
                }"
                :style="{
                  height: dualMenuShowText ? '60px' : '46px',
                }"
              >
                <ArtSvgIcon
                  class="menu-icon text-g-700 dark:text-g-800"
                  :icon="menu.meta.icon"
                  :style="{
                    marginBottom: dualMenuShowText ? '5px' : '0',
                  }"
                />
                <span v-if="dualMenuShowText" class="text-md text-g-700">
                  {{ $t(menu.meta.title) }}
                </span>
                <div
                  v-if="menu.meta.showBadge"
                  class="art-badge art-badge-dual"
                />
              </div>
            </ElTooltip>
          </li>
        </ul>
      </ElScrollbar>

      <ArtIconButton
        class="switch-btn size-10"
        icon="ri:arrow-left-right-fill"
        @click="toggleDualMenuMode"
      />
    </div>

    <div
      v-show="menuList.length > 0"
      class="menu-left"
      :class="`menu-left-${getMenuTheme.theme} menu-left-${!menuOpen ? 'close' : 'open'}`"
      :style="{ background: getMenuTheme.background }"
    >
      <ElScrollbar :style="scrollbarStyle">
        <div
          class="header"
          :style="{
            background: getMenuTheme.background,
          }"
          @click="navigateToHome"
        >
          <ArtLogo v-if="!isDualMenu" class="logo" />

          <p
            :class="{ 'is-dual-menu-name': isDualMenu }"
            :style="{
              color: getMenuTheme.systemNameColor,
              opacity: !menuOpen ? 0 : 1,
            }"
          >
            {{ siteName }}
          </p>
        </div>

        <ElMenu
          :class="'el-menu-' + getMenuTheme.theme"
          :collapse="!menuOpen"
          :default-active="routerPath"
          :text-color="getMenuTheme.textColor"
          :background-color="getMenuTheme.background"
          :default-openeds="defaultOpenedMenus"
          :popper-class="`menu-left-popper menu-left-${getMenuTheme.theme}-popper`"
          :show-timeout="50"
          :hide-timeout="50"
        >
          <SidebarSubmenu
            :list="menuList"
            :is-mobile="isMobileMode"
            :theme="getMenuTheme"
            @close="handleMenuClose"
          />
        </ElMenu>
      </ElScrollbar>

      <div
        v-if="isDualMenu"
        class="dual-menu-collapse-btn"
        @click="toggleMenuVisibility"
      >
        <ArtSvgIcon
          class="text-g-500/70"
          :icon="
            menuOpen ? 'ri:arrow-left-wide-fill' : 'ri:arrow-right-wide-fill'
          "
        />
      </div>

      <div
        class="menu-model"
        :style="{
          opacity: !menuOpen ? 0 : 1,
          transform: showMobileModal ? 'scale(1)' : 'scale(0)',
        }"
        @click="toggleMenuVisibility"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { useSettingStore } from "@/store/modules/setting";
import { MenuTypeEnum, MenuWidth } from "@/enums/appEnum";
import { useMenuStore } from "@/store/modules/menu";
import { isIframe } from "@/utils/navigation";
import { handleMenuJump } from "@/utils/navigation";
import SidebarSubmenu from "./widget/SidebarSubmenu.vue";
import { useCommon } from "@/hooks/core/useCommon";
import { useWindowSize, useTimeoutFn } from "@vueuse/core";
defineOptions({ name: "ArtSidebarMenu" });
const MOBILE_BREAKPOINT = 800;
const ANIMATION_DELAY = 350;
const MENU_CLOSE_WIDTH = MenuWidth.CLOSE;
const route = useRoute();
const router = useRouter();
const settingStore = useSettingStore();
const { getMenuOpenWidth, menuType, dualMenuShowText, menuOpen, getMenuTheme } =
  storeToRefs(settingStore);
const { siteName } = storeToRefs(settingStore);

const defaultOpenedMenus = ref<string[]>([
  "/ops",
  "/monitoring",
  "/control-panel",
]);
const isMobileMode = ref(false);
const showMobileModal = ref(false);
const { width } = useWindowSize();
const menuopenwidth = computed(() => getMenuOpenWidth.value);
const menuclosewidth = computed(() => MENU_CLOSE_WIDTH);
const isTopLeftMenu = computed(() => menuType.value === MenuTypeEnum.TOP_LEFT);
const showLeftMenu = computed(
  () =>
    menuType.value === MenuTypeEnum.LEFT ||
    menuType.value === MenuTypeEnum.TOP_LEFT,
);
const isDualMenu = computed(() => menuType.value === MenuTypeEnum.DUAL_MENU);
const isMobileScreen = computed(() => width.value < MOBILE_BREAKPOINT);
const firstLevelMenuPath = computed(() => route.matched[0]?.path);
const routerPath = computed(() => String(route.meta.activePath || route.path));
const firstLevelMenus = computed(() => {
  return useMenuStore().menuList.filter((menu) => !menu.meta.isHide);
});
const menuList = computed(() => {
  const menuStore = useMenuStore();
  const allMenus = menuStore.menuList;
  if (!isTopLeftMenu.value && !isDualMenu.value) {
    return allMenus;
  }
  if (isIframe(route.path)) {
    return findIframeMenuList(route.path, allMenus);
  }
  if (route.meta.isFirstLevel) {
    return [];
  }
  const currentTopPath = `/${route.path.split("/")[1]}`;
  const currentMenu = allMenus.find((menu) => menu.path === currentTopPath);
  return currentMenu?.children ?? [];
});
const scrollbarStyle = computed(() => {
  const isCollapsed = isDualMenu.value && !menuOpen.value;
  return {
    transform: isCollapsed ? "translateY(-50px)" : "translateY(0)",
    height: isCollapsed ? "calc(100% + 50px)" : "100%",
    transition: "transform 0.3s ease",
  };
});
const { start: delayHideMobileModal } = useTimeoutFn(
  () => {
    showMobileModal.value = false;
  },
  ANIMATION_DELAY,
  { immediate: false },
);
const findIframeMenuList = (currentPath: string, menuList: any[]) => {
  const hasPath = (items: any[]): boolean => {
    for (const item of items) {
      if (item.path === currentPath) {
        return true;
      }
      if (item.children && hasPath(item.children)) {
        return true;
      }
    }
    return false;
  };
  for (const menu of menuList) {
    if (menu.children && hasPath(menu.children)) {
      return menu.children;
    }
  }
  return [];
};
const { homePath } = useCommon();
const navigateToHome = (): void => {
  router.push(homePath.value);
};
const toggleMenuVisibility = (): void => {
  settingStore.setMenuOpen(!menuOpen.value);
  if (isMobileScreen.value) {
    if (!menuOpen.value) {
      showMobileModal.value = true;
    } else {
      delayHideMobileModal();
    }
  }
};
const handleMenuClose = (): void => {
  if (isMobileScreen.value) {
    settingStore.setMenuOpen(false);
    delayHideMobileModal();
  }
};
const toggleDualMenuMode = (): void => {
  settingStore.setDualMenuShowText(!dualMenuShowText.value);
};
watch(width, (newWidth) => {
  if (newWidth < MOBILE_BREAKPOINT) {
    settingStore.setMenuOpen(false);
    if (!menuOpen.value) {
      showMobileModal.value = false;
    }
  } else {
    showMobileModal.value = false;
  }
});
watch(menuOpen, (isMenuOpen: boolean) => {
  if (!isMobileScreen.value) {
    showMobileModal.value = false;
  } else {
    if (isMenuOpen) {
      showMobileModal.value = true;
    } else {
      delayHideMobileModal();
    }
  }
});
</script>

<style lang="scss" scoped>
@use "./style";
</style>

<style lang="scss">
@use "./theme";

.layout-sidebar {
  .el-menu:not(.el-menu--collapse) {
    width: v-bind(menuopenwidth);
  }

  .el-menu--collapse {
    width: v-bind(menuclosewidth);
  }
}
</style>
