<template>
  <div class="relative box-border flex-c w-full overflow-hidden">
    <div v-show="showLeftArrow" class="button-arrow" @click="scroll('left')">
      <ElIcon>
        <ArrowLeft />
      </ElIcon>
    </div>

    <ElScrollbar
      ref="scrollbarRef"
      wrap-class="scrollbar-wrapper"
      :horizontal="true"
      @scroll="handleScroll"
      @wheel="handleWheel"
    >
      <div
        class="box-border flex-c flex-shrink-0 flex-nowrap h-12 whitespace-nowrap"
      >
        <template v-for="item in processedMenuList" :key="item.meta.title">
          <div
            v-if="!item.meta.isHide"
            class="menu-item relative flex-shrink-0 h-10 px-3 text-sm flex-c c-p hover:text-theme"
            :class="{
              'menu-item-active text-theme': item.isActive,
            }"
            @click="handleMenuJump(item, true)"
          >
            <ArtSvgIcon
              :icon="item.meta.icon"
              class="text-lg text-g-700 dark:text-g-800 mr-1"
              :class="item.isActive && '!text-theme'"
            />
            <span
              class="text-md text-g-700 dark:text-g-800"
              :class="item.isActive && '!text-theme'"
            >
              {{ item.formattedTitle }}
            </span>
            <div v-if="item.meta.showBadge" class="art-badge art-badge-mixed" />
          </div>
        </template>
      </div>
    </ElScrollbar>

    <div
      v-show="showRightArrow"
      class="button-arrow right-2"
      @click="scroll('right')"
    >
      <ElIcon>
        <ArrowRight />
      </ElIcon>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, nextTick } from "vue";
import { ArrowLeft, ArrowRight } from "@element-plus/icons-vue";
import { useThrottleFn } from "@vueuse/core";
import { formatMenuTitle } from "@/utils/router";
import { handleMenuJump } from "@/utils/navigation";
import type { AppRouteRecord } from "@/types/router";
defineOptions({ name: "ArtMixedMenu" });
interface Props {
  list: AppRouteRecord[];
}
interface ProcessedMenuItem extends AppRouteRecord {
  isActive: boolean;
  formattedTitle: string;
}
type ScrollDirection = "left" | "right";
const route = useRoute();
const props = withDefaults(defineProps<Props>(), {
  list: () => [],
});
const scrollbarRef = ref<any>();
const showLeftArrow = ref(false);
const showRightArrow = ref(false);
const SCROLL_CONFIG = {
  BUTTON_SCROLL_DISTANCE: 200,
  WHEEL_FAST_STEP: 35,
  WHEEL_SLOW_STEP: 30,
  WHEEL_FAST_THRESHOLD: 100,
};
const currentActivePath = computed(() => {
  return String(route.meta.activePath || route.path);
});
const isMenuItemActive = (item: AppRouteRecord): boolean => {
  const activePath = currentActivePath.value;
  if (item.children?.length) {
    return item.children.some((child) => {
      if (child.children?.length) {
        return isMenuItemActive(child);
      }
      return child.path === activePath;
    });
  }
  return item.path === activePath;
};
const processedMenuList = computed<ProcessedMenuItem[]>(() => {
  return props.list.map((item) => ({
    ...item,
    isActive: isMenuItemActive(item),
    formattedTitle: formatMenuTitle(item.meta.title),
  }));
});
const handleScrollCore = (): void => {
  if (!scrollbarRef.value?.wrapRef) return;
  const { scrollLeft, scrollWidth, clientWidth } = scrollbarRef.value.wrapRef;
  showLeftArrow.value = scrollLeft > 0;
  showRightArrow.value = scrollLeft + clientWidth < scrollWidth;
};
const handleScroll = useThrottleFn(handleScrollCore, 16);
const scroll = (direction: ScrollDirection): void => {
  if (!scrollbarRef.value?.wrapRef) return;
  const currentScroll = scrollbarRef.value.wrapRef.scrollLeft;
  const targetScroll =
    direction === "left"
      ? currentScroll - SCROLL_CONFIG.BUTTON_SCROLL_DISTANCE
      : currentScroll + SCROLL_CONFIG.BUTTON_SCROLL_DISTANCE;
  scrollbarRef.value.wrapRef.scrollTo({
    left: targetScroll,
    behavior: "smooth",
  });
};
const handleWheel = (event: WheelEvent): void => {
  event.preventDefault();
  event.stopPropagation();
  if (!scrollbarRef.value?.wrapRef) return;
  const { wrapRef } = scrollbarRef.value;
  const { scrollLeft, scrollWidth, clientWidth } = wrapRef;
  const scrollStep =
    Math.abs(event.deltaY) > SCROLL_CONFIG.WHEEL_FAST_THRESHOLD
      ? SCROLL_CONFIG.WHEEL_FAST_STEP
      : SCROLL_CONFIG.WHEEL_SLOW_STEP;
  const scrollDelta = event.deltaY > 0 ? scrollStep : -scrollStep;
  const targetScroll = Math.max(
    0,
    Math.min(scrollLeft + scrollDelta, scrollWidth - clientWidth),
  );
  wrapRef.scrollLeft = targetScroll;
  handleScrollCore();
};
const initScrollState = (): void => {
  nextTick(() => {
    handleScrollCore();
  });
};
onMounted(initScrollState);
</script>

<style scoped>
@reference '@styles/core/tailwind.css';

.button-arrow {
  @apply absolute
    top-1/2
    z-2
    flex
    items-center
    justify-center
    size-7.5
    text-g-600
    cursor-pointer
    rounded
    transition-all
    duration-300
    -translate-y-1/2
    hover:text-g-900
    hover:bg-g-200;
}
</style>

<style scoped>
:deep(.el-scrollbar__bar.is-horizontal) {
  bottom: 5px;
  display: none;
  height: 2px;
}

:deep(.scrollbar-wrapper) {
  flex: 1;
  min-width: 0;
  margin: 0 50px 0 30px;
}

.menu-item-active::after {
  position: absolute;
  right: 0;
  bottom: 0;
  left: 0;
  width: 40px;
  height: 2px;
  margin: auto;
  content: "";
  background-color: var(--theme-color);
}

@media (width <= 1440px) {
  :deep(.scrollbar-wrapper) {
    margin: 0 45px;
  }
}
</style>
