<template>
  <div class="app-layout" :class="{ 'is-full-page': isFullPage }">
    <aside v-show="!isFullPage" id="app-sidebar">
      <ArtSidebarMenu />
    </aside>

    <main id="app-main">
      <div v-show="!isFullPage" id="app-header">
        <ArtHeaderBar />
      </div>
      <div id="app-content">
        <ArtPageContent />
      </div>
    </main>

    <div v-show="!isFullPage" id="app-global">
      <ArtGlobalComponent />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useRoute } from "vue-router";

defineOptions({ name: "AppLayout" });

const route = useRoute();
const isFullPage = computed(() =>
  route.matched.some((r) => r.meta?.isFullPage === true),
);
</script>

<style lang="scss" scoped>
@use "./style";

.app-layout.is-full-page {
  #app-main,
  #app-content {
    width: 100%;
    max-width: 100%;
    margin: 0;
    padding: 0;
  }
}
</style>
