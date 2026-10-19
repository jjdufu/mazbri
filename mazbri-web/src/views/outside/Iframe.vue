<template>
  <div v-loading="isLoading" class="box-border w-full h-full">
    <iframe
      ref="iframeRef"
      :src="iframeUrl"
      frameborder="0"
      class="w-full h-full min-h-[calc(100vh-120px)] border-none"
      @load="handleIframeLoad"
    ></iframe>
  </div>
</template>

<script setup lang="ts">
import { IframeRouteManager } from "@/router/core";
defineOptions({ name: "IframeView" });
const route = useRoute();
const isLoading = ref(true);
const iframeUrl = ref("");
const iframeRef = ref<HTMLIFrameElement | null>(null);
onMounted(() => {
  const iframeRoute = IframeRouteManager.getInstance().findByPath(route.path);
  if (iframeRoute?.meta) {
    iframeUrl.value = iframeRoute.meta.link || "";
  }
});
const handleIframeLoad = (): void => {
  isLoading.value = false;
};
</script>
