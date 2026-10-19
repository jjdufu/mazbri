<template>
  <div
    class="w-full bg-[var(--default-bg-color)]"
    :class="[
      tabStyle === 'tab-card' || tabStyle === 'tab-google'
        ? 'mb-5 max-sm:mb-3 !bg-box'
        : '',
    ]"
  >
    <div
      class="relative box-border flex-b h-15 leading-15 select-none"
      :class="[
        tabStyle === 'tab-card' || tabStyle === 'tab-google'
          ? 'border-b border-[var(--art-card-border)]'
          : '',
      ]"
    >
      <div class="flex-c flex-1 min-w-0 leading-15" style="display: flex">
        <div v-if="isTopMenu" class="flex-c c-p" @click="toHome">
          <ArtLogo class="pl-4.5" />
          <p v-if="width >= 1400" class="my-0 mx-2 ml-2 text-lg">
            {{ siteName }}
          </p>
        </div>

        <ArtLogo
          class="!hidden pl-3.5 overflow-hidden align-[-0.15em] fill-current"
          @click="toHome"
        />

        <ArtIconButton
          v-if="isLeftMenu && shouldShowMenuButton"
          icon="ri:menu-2-fill"
          class="ml-3 max-sm:ml-[7px]"
          @click="visibleMenu"
        />

        <ArtIconButton
          v-if="shouldShowRefreshButton"
          icon="ri:refresh-line"
          class="!ml-3 refresh-btn max-sm:!hidden"
          :style="{ marginLeft: !isLeftMenu ? '10px' : '0' }"
          @click="reload"
        />

        <ArtFastEnter
          v-if="shouldShowFastEnter && width >= headerBarFastEnterMinWidth"
        >
          <ArtIconButton icon="ri:function-line" class="ml-3" />
        </ArtFastEnter>

        <ArtBreadcrumb
          v-if="
            (shouldShowBreadcrumb && isLeftMenu) ||
            (shouldShowBreadcrumb && isDualMenu)
          "
        />

        <ArtHorizontalMenu v-if="isTopMenu" :list="menuList" />

        <ArtMixedMenu v-if="isTopLeftMenu" :list="menuList" />
      </div>

      <div class="flex-c gap-2.5">
        <ArtIconButton
          icon="ri:lock-line"
          class="lock-screen-btn ml-3 max-md:!hidden"
          @click="lockScreen"
        />

        <ElDropdown
          v-if="shouldShowLanguage"
          popper-class="langDropDownStyle"
          @command="changeLanguage"
        >
          <ArtIconButton
            icon="ri:translate-2"
            class="language-btn text-[19px]"
          />
          <template #dropdown>
            <ElDropdownMenu>
              <div
                v-for="item in languageOptions"
                :key="item.value"
                class="lang-btn-item"
              >
                <ElDropdownItem
                  :command="item.value"
                  :class="{ 'is-selected': locale === item.value }"
                >
                  <span class="menu-txt">{{ $t(item.labelKey) }}</span>
                  <ArtSvgIcon
                    v-if="locale === item.value"
                    icon="ri:check-fill"
                  />
                </ElDropdownItem>
              </div>
            </ElDropdownMenu>
          </template>
        </ElDropdown>

        <ArtIconButton
          v-if="shouldShowThemeToggle"
          :icon="isDark ? 'ri:sun-fill' : 'ri:moon-line'"
          @click="handleThemeToggle"
        />

        <ArtUserMenu />
      </div>
    </div>

    <ArtWorkTab />
  </div>
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import { useWindowSize } from "@vueuse/core";
import { LanguageEnum, MenuTypeEnum, SystemThemeEnum } from "@/enums/appEnum";
import { useSettingStore } from "@/store/modules/setting";
import { useUserStore } from "@/store/modules/user";
import { useMenuStore } from "@/store/modules/menu";
import { languageOptions } from "@/locales";
import { mittBus } from "@/utils/sys";
import { triggerLockScreen } from "@/utils/sys/global-ui";
import { useCommon } from "@/hooks/core/useCommon";
import { useHeaderBar } from "@/hooks/core/useHeaderBar";
import { useTheme } from "@/hooks/core/useTheme";
import ArtUserMenu from "./widget/ArtUserMenu.vue";
defineOptions({ name: "ArtHeaderBar" });
const router = useRouter();
const { locale } = useI18n();
const { width } = useWindowSize();
const settingStore = useSettingStore();
const userStore = useUserStore();
const menuStore = useMenuStore();
const {
  shouldShowMenuButton,
  shouldShowRefreshButton,
  shouldShowFastEnter,
  shouldShowBreadcrumb,
  shouldShowLanguage,
  shouldShowThemeToggle,
  fastEnterMinWidth: headerBarFastEnterMinWidth,
} = useHeaderBar();
const { menuOpen, menuType, isDark, tabStyle } = storeToRefs(settingStore);
const { siteName } = storeToRefs(settingStore);
const { language } = storeToRefs(userStore);
const { menuList } = storeToRefs(menuStore);
const { switchThemeStylesAndPersist } = useTheme();
const isLeftMenu = computed(() => menuType.value === MenuTypeEnum.LEFT);
const isDualMenu = computed(() => menuType.value === MenuTypeEnum.DUAL_MENU);
const isTopMenu = computed(() => menuType.value === MenuTypeEnum.TOP);
const isTopLeftMenu = computed(() => menuType.value === MenuTypeEnum.TOP_LEFT);
onMounted(() => {
  initLanguage();
});
const visibleMenu = (): void => {
  settingStore.setMenuOpen(!menuOpen.value);
};
const { homePath } = useCommon();
const { refresh } = useCommon();
const toHome = (): void => {
  router.push(homePath.value);
};
const handleThemeToggle = (): void => {
  const currentTheme = document.documentElement.classList.contains("dark")
    ? SystemThemeEnum.DARK
    : SystemThemeEnum.LIGHT;
  const nextTheme =
    currentTheme === SystemThemeEnum.DARK
      ? SystemThemeEnum.LIGHT
      : SystemThemeEnum.DARK;
  if (document.startViewTransition) {
    document.startViewTransition(() => {
      void switchThemeStylesAndPersist(nextTheme);
    });
    return;
  }
  void switchThemeStylesAndPersist(nextTheme);
};
const reload = (time: number = 0): void => {
  setTimeout(() => {
    refresh();
  }, time);
};
const initLanguage = (): void => {
  locale.value = language.value;
};
const changeLanguage = (lang: LanguageEnum): void => {
  if (locale.value === lang) return;
  locale.value = lang;
  userStore.setLanguage(lang);
  reload(50);
};
const lockScreen = (): void => {
  triggerLockScreen();
  mittBus.emit("openLockScreen");
};
</script>

<style lang="scss" scoped>
@keyframes rotate180 {
  0% {
    transform: rotate(0);
  }

  100% {
    transform: rotate(180deg);
  }
}

@keyframes shake {
  0% {
    transform: rotate(0);
  }

  25% {
    transform: rotate(-5deg);
  }

  50% {
    transform: rotate(5deg);
  }

  75% {
    transform: rotate(-5deg);
  }

  100% {
    transform: rotate(0);
  }
}

@keyframes expand {
  0% {
    transform: scale(1);
  }

  50% {
    transform: scale(1.1);
  }

  100% {
    transform: scale(1);
  }
}

@keyframes shrink {
  0% {
    transform: scale(1);
  }

  50% {
    transform: scale(0.9);
  }

  100% {
    transform: scale(1);
  }
}

@keyframes moveUp {
  0% {
    transform: translateY(0);
  }

  50% {
    transform: translateY(-3px);
  }

  100% {
    transform: translateY(0);
  }
}

@keyframes breathing {
  0% {
    opacity: 0.4;
    transform: scale(0.9);
  }

  50% {
    opacity: 1;
    transform: scale(1.1);
  }

  100% {
    opacity: 0.4;
    transform: scale(0.9);
  }
}

.refresh-btn:hover :deep(.art-svg-icon) {
  animation: rotate180 0.5s;
}

.language-btn:hover :deep(.art-svg-icon) {
  animation: moveUp 0.4s;
}

.setting-btn:hover :deep(.art-svg-icon) {
  animation: rotate180 0.5s;
}

.full-screen-btn:hover :deep(.art-svg-icon) {
  animation: expand 0.6s forwards;
}

.exit-full-screen-btn:hover :deep(.art-svg-icon) {
  animation: shrink 0.6s forwards;
}

@media screen and (width <= 768px) {
  .logo2 {
    display: block !important;
  }
}

@media screen and (width <= 640px) {
  .btn-box {
    width: 40px;
  }
}
</style>
