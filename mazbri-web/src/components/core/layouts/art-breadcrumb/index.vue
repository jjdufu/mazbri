<template>
  <nav class="ml-2.5 max-lg:!hidden" aria-label="breadcrumb">
    <ul class="flex-c h-full">
      <li
        v-for="(item, index) in breadcrumbItems"
        :key="item.path"
        class="box-border flex-c h-7 text-sm leading-7"
      >
        <div
          :class="
            isClickable(item, index)
              ? 'c-p py-1 rounded tad-200 hover:bg-active-color hover:[&_span]:text-g-600'
              : ''
          "
          @click="handleBreadcrumbClick(item, index)"
        >
          <span
            class="block max-w-46 overflow-hidden text-ellipsis whitespace-nowrap px-1.5 text-sm text-g-600 dark:text-g-800"
            >{{ formatMenuTitle(item.meta?.title as string) }}</span
          >
        </div>
        <div
          v-if="!isLastItem(index) && item.meta?.title"
          class="mx-1 text-sm not-italic text-g-500"
          aria-hidden="true"
        >
          /
        </div>
      </li>
    </ul>
  </nav>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useRouter, useRoute } from "vue-router";
import type { RouteLocationMatched, RouteRecordRaw } from "vue-router";
import { ErrorCodes } from "@/constants/error-codes";
import { formatMenuTitle } from "@/utils/router";
import { logger } from "@/utils/sys";
defineOptions({ name: "ArtBreadcrumb" });
export interface BreadcrumbItem {
  path: string;
  meta: RouteRecordRaw["meta"];
}
const route = useRoute();
const router = useRouter();
const breadcrumbItems = computed<BreadcrumbItem[]>(() => {
  const { matched } = route;
  const matchedLength = matched.length;
  if (!matchedLength || isHomeRoute(matched[0])) {
    return [];
  }
  const firstRoute = matched[0];
  const isFirstLevel = firstRoute.meta?.isFirstLevel;
  const lastIndex = matchedLength - 1;
  const currentRoute = matched[lastIndex];
  const currentRouteMeta = currentRoute.meta;
  let items = isFirstLevel
    ? [createBreadcrumbItem(currentRoute)]
    : matched.map(createBreadcrumbItem);
  if (items.length > 1 && isWrapperContainer(items[0])) {
    items = items.slice(1);
  }
  if (
    currentRouteMeta?.isIframe &&
    (items.length === 1 || items.every(isWrapperContainer))
  ) {
    return [createBreadcrumbItem(currentRoute)];
  }
  return items;
});
const isWrapperContainer = (item: BreadcrumbItem): boolean =>
  item.path === "/outside" && !!item.meta?.isIframe;
const createBreadcrumbItem = (route: RouteLocationMatched): BreadcrumbItem => ({
  path: route.path,
  meta: route.meta,
});
const isHomeRoute = (route: RouteLocationMatched): boolean =>
  route.name === "/";
const isLastItem = (index: number): boolean => {
  const itemsLength = breadcrumbItems.value.length;
  return index === itemsLength - 1;
};
const isClickable = (item: BreadcrumbItem, index: number): boolean =>
  item.path !== "/outside" && !isLastItem(index);
const findFirstValidChild = (route: RouteRecordRaw) =>
  route.children?.find((child) => !child.redirect && !child.meta?.isHide);
const buildFullPath = (childPath: string): string =>
  `/${childPath}`.replace("//", "/");
async function handleBreadcrumbClick(
  item: BreadcrumbItem,
  index: number,
): Promise<void> {
  if (isLastItem(index) || item.path === "/outside") {
    return;
  }
  try {
    const routes = router.getRoutes();
    const targetRoute = routes.find((route) => route.path === item.path);
    if (!targetRoute?.children?.length) {
      await router.push(item.path);
      return;
    }
    const firstValidChild = findFirstValidChild(targetRoute);
    if (firstValidChild) {
      await router.push(buildFullPath(firstValidChild.path));
    } else {
      await router.push(item.path);
    }
  } catch (error) {
    logger.error("Breadcrumb navigation failed", {
      service: "mazbri-mgmt-web",
      event: "breadcrumb_navigation_failed",
      severity: "error",
      error_code: ErrorCodes.INTERNAL_ERROR,
      target_path: item.path,
      error,
    });
  }
}
</script>
