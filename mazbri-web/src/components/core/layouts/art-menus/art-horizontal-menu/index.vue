<template>
  <div class="flex-1 overflow-hidden">
    <ElMenu
      :ellipsis="true"
      mode="horizontal"
      :default-active="routerPath"
      :text-color="isDark ? 'var(--art-gray-800)' : 'var(--art-gray-700)'"
      :popper-offset="-6"
      background-color="transparent"
      :show-timeout="50"
      :hide-timeout="50"
      popper-class="horizontal-menu-popper"
      class="w-full border-none"
    >
      <HorizontalSubmenu
        v-for="item in filteredMenuItems"
        :key="item.path"
        :item="item"
        :is-mobile="false"
        :level="0"
      />
    </ElMenu>
  </div>
</template>

<script setup lang="ts">
import type { AppRouteRecord } from "@/types/router";
import HorizontalSubmenu from "./widget/HorizontalSubmenu.vue";
import { useSettingStore } from "@/store/modules/setting";
defineOptions({ name: "ArtHorizontalMenu" });
const settingStore = useSettingStore();
const { isDark } = storeToRefs(settingStore);
interface Props {
  list: AppRouteRecord[];
}
const route = useRoute();
const props = withDefaults(defineProps<Props>(), {
  list: () => [],
});
const filteredMenuItems = computed(() => {
  return filterMenuItems(props.list);
});
const routerPath = computed(() => String(route.meta.activePath || route.path));
const filterMenuItems = (items: AppRouteRecord[]): AppRouteRecord[] => {
  return items
    .filter((item) => {
      if (item.meta.isHide) {
        return false;
      }
      if (item.children && item.children.length > 0) {
        const filteredChildren = filterMenuItems(item.children);
        return filteredChildren.length > 0;
      }
      return true;
    })
    .map((item) => ({
      ...item,
      children: item.children ? filterMenuItems(item.children) : undefined,
    }));
};
</script>

<style scoped>
:deep(.el-menu) {
  border-bottom: none !important;
}

:deep(.el-menu-item[tabindex="0"]) {
  background-color: transparent !important;
  border: none !important;
}

:deep(.el-menu--horizontal .el-sub-menu__title) {
  padding: 0 30px 0 10px !important;
  border: 0 !important;
}
</style>
