<template>
  <div
    class="layout-content"
    :class="{ 'overflow-auto': isFullPage }"
    :style="containerStyle"
  >
    <div id="app-content-header">
      <ArtFestivalTextScroll v-if="!isFullPage" />

      <div
        v-if="isOpenRouteInfo === 'true'"
        class="px-2 py-1.5 mb-3 text-sm text-g-500 bg-g-200 border-full-d rounded-md"
      >
        router meta：{{ route.meta }}
      </div>
    </div>

    <RouterView
      v-if="isRefresh"
      v-slot="{ Component, route: currentRoute }"
      :style="contentStyle"
    >
      <Transition
        :name="showTransitionMask ? '' : actualTransition"
        mode="out-in"
        appear
      >
        <KeepAlive :max="10" :exclude="keepAliveExclude">
          <component
            :is="Component"
            v-if="currentRoute.meta.keepAlive"
            :key="currentRoute.path"
            class="art-page-view"
          />
        </KeepAlive>
      </Transition>

      <Transition
        :name="showTransitionMask ? '' : actualTransition"
        mode="out-in"
        appear
      >
        <component
          :is="Component"
          v-if="!currentRoute.meta.keepAlive"
          :key="currentRoute.path"
          class="art-page-view"
        />
      </Transition>
    </RouterView>

    <Teleport to="body">
      <div
        v-show="showTransitionMask"
        class="fixed top-0 left-0 z-[2000] w-screen h-screen pointer-events-none bg-box"
      />
    </Teleport>
  </div>
</template>
<script setup lang="ts">
import type { CSSProperties } from "vue";
import { useRoute } from "vue-router";
import { useAutoLayoutHeight } from "@/hooks/core/useLayoutHeight";
import { useSettingStore } from "@/store/modules/setting";
import { useWorktabStore } from "@/store/modules/worktab";
defineOptions({ name: "ArtPageContent" });
const route = useRoute();
const { containerMinHeight } = useAutoLayoutHeight();
const { pageTransition, containerWidth, refresh } =
  storeToRefs(useSettingStore());
const { keepAliveExclude } = storeToRefs(useWorktabStore());
const isRefresh = shallowRef(true);
const isOpenRouteInfo = import.meta.env.VITE_OPEN_ROUTE_INFO;
const showTransitionMask = ref(false);
const isFirstLoad = ref(true);
const isFullPage = computed(() =>
  route.matched.some((r) => r.meta?.isFullPage),
);
const prevIsFullPage = ref(isFullPage.value);
const actualTransition = computed(() => {
  if (isFirstLoad.value) return "";
  if (prevIsFullPage.value && !isFullPage.value) return "";
  return pageTransition.value;
});
watch(isFullPage, (val, oldVal) => {
  if (val !== oldVal) {
    showTransitionMask.value = true;
    setTimeout(() => {
      showTransitionMask.value = false;
    }, 50);
  }
  nextTick(() => {
    prevIsFullPage.value = val;
  });
});
const containerStyle = computed(
  (): CSSProperties =>
    isFullPage.value
      ? {
          position: "fixed",
          top: 0,
          left: 0,
          width: "100%",
          height: "100vh",
          zIndex: 2500,
          background: "var(--default-bg-color)",
        }
      : {
          maxWidth: containerWidth.value,
        },
);
const contentStyle = computed(
  (): CSSProperties => ({
    minHeight: containerMinHeight.value,
  }),
);
const reload = () => {
  isRefresh.value = false;
  nextTick(() => {
    isRefresh.value = true;
  });
};
watch(refresh, reload, { flush: "post" });
onMounted(() => {
  nextTick(() => {
    isFirstLoad.value = false;
  });
});
</script>
