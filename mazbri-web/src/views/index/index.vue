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
import { computed, nextTick, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { usePersonalSecurityStore } from "@/store/modules/personal-security";

defineOptions({ name: "AppLayout" });

const route = useRoute();
const router = useRouter();
const personalSecurityStore = usePersonalSecurityStore();
const isFullPage = computed(() =>
  route.matched.some((r) => r.meta?.isFullPage === true),
);

watch(
  () => route.query.setupMfa,
  async (setupMfa) => {
    const flag = Array.isArray(setupMfa) ? setupMfa[0] : setupMfa;
    if (flag !== "1") return;
    personalSecurityStore.openDrawer("mfa");
    await nextTick();
    const current = router.currentRoute.value;
    if (!("setupMfa" in current.query)) return;
    const nextQuery = { ...current.query };
    delete nextQuery.setupMfa;
    void router.replace({
      path: current.path,
      query: nextQuery,
      hash: current.hash,
    });
  },
  { immediate: true },
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
