<template>
  <template
    v-for="(item, index) in filteredMenuItems"
    :key="getUniqueKey(item, index)"
  >
    <ElMenuItemGroup
      v-if="level === 0 && hasChildren(item)"
      class="sidebar-section"
    >
      <template #title>
        <span class="sidebar-section__title">
          {{ formatMenuTitle(item.meta.title) }}
        </span>
      </template>

      <SidebarSubmenu
        :list="item.children"
        :is-mobile="isMobile"
        :level="level + 1"
        :theme="theme"
        @close="closeMenu"
      />
    </ElMenuItemGroup>

    <ElSubMenu
      v-else-if="hasChildren(item)"
      :index="item.path || item.meta.title"
      :level="level"
    >
      <template #title>
        <div class="menu-icon flex-cc">
          <ArtSvgIcon
            :icon="item.meta.icon"
            :color="theme?.iconColor"
            :style="{ color: theme.iconColor }"
          />
        </div>
        <span class="menu-name">
          {{ formatMenuTitle(item.meta.title) }}
        </span>
        <div v-if="item.meta.showBadge" class="art-badge" style="right: 10px" />
      </template>

      <SidebarSubmenu
        :list="item.children"
        :is-mobile="isMobile"
        :level="level + 1"
        :theme="theme"
        @close="closeMenu"
      />
    </ElSubMenu>

    <ElMenuItem
      v-else
      :index="isExternalLink(item) ? undefined : item.path || item.meta.title"
      :level-item="level + 1"
      @click="goPage(item)"
    >
      <div class="menu-icon flex-cc">
        <ArtSvgIcon
          :icon="item.meta.icon"
          :color="theme?.iconColor"
          :style="{ color: theme.iconColor }"
        />
      </div>
      <div
        v-show="item.meta.showBadge && level === 0 && !menuOpen"
        class="art-badge"
        style="right: 5px"
      />

      <template #title>
        <span class="menu-name">
          {{ formatMenuTitle(item.meta.title) }}
        </span>
        <div v-if="item.meta.showBadge" class="art-badge" />
        <div
          v-if="item.meta.showTextBadge && (level > 0 || menuOpen)"
          class="art-text-badge"
        >
          {{ item.meta.showTextBadge }}
        </div>
      </template>
    </ElMenuItem>
  </template>
</template>

<script setup lang="ts">
import { computed } from "vue";
import type { AppRouteRecord } from "@/types/router";
import { formatMenuTitle } from "@/utils/router";
import { handleMenuJump } from "@/utils/navigation";
import { useSettingStore } from "@/store/modules/setting";
interface MenuTheme {
  iconColor?: string;
}
interface Props {
  title?: string;
  list?: AppRouteRecord[];
  theme?: MenuTheme;
  isMobile?: boolean;
  level?: number;
}
interface Emits {
  (e: "close"): void;
}
const props = withDefaults(defineProps<Props>(), {
  title: "",
  list: () => [],
  theme: () => ({}),
  isMobile: false,
  level: 0,
});
const emit = defineEmits<Emits>();
const settingStore = useSettingStore();
const { menuOpen } = storeToRefs(settingStore);
const filteredMenuItems = computed(() => filterRoutes(props.list));
const goPage = (item: AppRouteRecord): void => {
  closeMenu();
  handleMenuJump(item);
};
const closeMenu = (): void => {
  emit("close");
};
const filterRoutes = (items: AppRouteRecord[]): AppRouteRecord[] => {
  return items
    .filter((item) => {
      if (item.meta.isHide) {
        return false;
      }
      if (item.children && item.children.length > 0) {
        const filteredChildren = filterRoutes(item.children);
        return filteredChildren.length > 0;
      }
      return true;
    })
    .map((item) => ({
      ...item,
      children: item.children ? filterRoutes(item.children) : undefined,
    }));
};
const hasChildren = (item: AppRouteRecord): boolean => {
  if (!item.children || item.children.length === 0) {
    return false;
  }
  const filteredChildren = filterRoutes(item.children);
  return filteredChildren.length > 0;
};
const isExternalLink = (item: AppRouteRecord): boolean => {
  return !!(item.meta.link && !item.meta.isIframe);
};
const getUniqueKey = (item: AppRouteRecord, index: number): string => {
  return `${item.path || item.meta.title || "menu"}-${props.level}-${index}`;
};
</script>
